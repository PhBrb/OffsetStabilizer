use core::sync::atomic::{AtomicU32, Ordering};
use heapless::spsc::{Consumer, Producer, Queue};
use stm32h7xx_hal::dma::{
    bdma::{BdmaConfig, StreamX, StreamsTuple},
    DBTransfer, PeripheralToMemory, Transfer,
};
use stm32h7xx_hal::gpio::{Alternate, Pin};
use stm32h7xx_hal::prelude::*;
use stm32h7xx_hal::rcc::{
    rec::{Bdma, Spi6},
    CoreClocks,
};
use stm32h7xx_hal::spi::{Disabled, Enabled, Spi, MODE_1};
use stm32h7xx_hal::stm32::{
    spi1::cfg2::COMM_A, Interrupt, BDMA, EXTI, SPI6, SYSCFG,
};

const CHUNK_SIZE: usize = 32;
type Chunk = (u32, heapless::Vec<u8, CHUNK_SIZE>);
const CHUNK_QUEUE_SIZE: usize = 32;
static mut CHUNK_QUEUE: Queue<Chunk, CHUNK_QUEUE_SIZE> = Queue::new();

struct TransactionBorders {
    start_chunk: u32,
    start_byte: u8,
    end_chunk: u32,
    end_byte: u8,
}

static mut TXN_QUEUE: Queue<TransactionBorders, CHUNK_QUEUE_SIZE> =
    Queue::new();

#[link_section = ".sram4"]
pub static mut BDMA_BUF0: [u8; CHUNK_SIZE] = [0; CHUNK_SIZE];
#[link_section = ".sram4"]
pub static mut BDMA_BUF1: [u8; CHUNK_SIZE] = [0; CHUNK_SIZE];

static CUR_CHUNK_ID: AtomicU32 = AtomicU32::new(0);

pub enum KasliLinkError {
    NoMsgAvailable,
    BufferTooSmall,
    DesyncBody,
    DesyncTxn,
    BodyPartiallyMissing,
}

pub type KasliLinkResult<T> = core::result::Result<T, KasliLinkError>;

fn get_current_chunk_id() -> u32 {
    unsafe { CUR_CHUNK_ID.load(Ordering::Relaxed) }
}

fn fetch_add_chunk_id(inc: u32) -> u32 {
    unsafe { CUR_CHUNK_ID.fetch_add(inc, Ordering::Relaxed) }
}

fn get_chunk(id: u32) -> &'static mut [u8; CHUNK_SIZE] {
    if id % 2 == 0 {
        unsafe { &mut BDMA_BUF0 }
    } else {
        unsafe { &mut BDMA_BUF1 }
    }
}

fn get_current_chunk() -> &'static mut [u8; CHUNK_SIZE] {
    get_chunk(get_current_chunk_id())
}

impl TransactionBorders {
    fn size(&self) -> usize {
        CHUNK_SIZE * (self.end_chunk - self.start_chunk) as usize
            + self.end_byte as usize
            - self.start_byte as usize
    }
}

pub struct KasliLinkBdmaHandler {
    rx_xfer: Transfer<
        StreamX<BDMA, 0>,
        Spi<SPI6, Disabled>,
        PeripheralToMemory,
        &'static mut [u8; CHUNK_SIZE],
        DBTransfer,
    >,
    prod: Producer<'static, Chunk, CHUNK_QUEUE_SIZE>,
}

impl KasliLinkBdmaHandler {
    fn start(
        kasli_spi: Spi<SPI6, Disabled, u8>,
        bdma_rec: Bdma,
        bdma_periph: BDMA,
        prod: Producer<'static, Chunk, CHUNK_QUEUE_SIZE>,
    ) -> Self {
        let streams = StreamsTuple::new(bdma_periph, bdma_rec);
        let rx_stream = streams.0;
        let cfg = BdmaConfig::default()
            .memory_increment(true)
            .double_buffer(false)
            .transfer_complete_interrupt(true)
            .half_transfer_interrupt(false)
            .transfer_error_interrupt(true);

        let rx_xfer = unsafe {
            cortex_m::peripheral::NVIC::unmask(Interrupt::BDMA_CH1);
            let mut rx_xfer: Transfer<_, _, PeripheralToMemory, _, _> =
                Transfer::init(
                    rx_stream,
                    kasli_spi,
                    &mut BDMA_BUF0,
                    Some(&mut BDMA_BUF1),
                    cfg,
                );
            rx_xfer.start(|spi| {
                spi.enable_dma_rx();

                spi.inner().cr2.modify(|_, w| w.tsize().bits(0));
                spi.inner().cr1.modify(|_, w| w.spe().set_bit());
            });

            rx_xfer
        };

        Self { rx_xfer, prod }
    }

    pub fn handle_bdma(&mut self) {
        let chunk_id = fetch_add_chunk_id(1);

        let _ = self.rx_xfer.next_transfer(get_chunk(chunk_id + 1));

        let mut chunk = heapless::Vec::new();

        chunk.extend_from_slice(get_chunk(chunk_id)).unwrap();

        let _ = self.prod.enqueue((chunk_id, chunk));
    }
}

pub struct KasliLinkNssHandler {
    prod: Producer<'static, TransactionBorders, CHUNK_QUEUE_SIZE>,
    last_chunk: u32,
    last_byte: u8,
}

impl KasliLinkNssHandler {
    fn start(
        prod: Producer<'static, TransactionBorders, CHUNK_QUEUE_SIZE>,
    ) -> Self {
        let chunk_id = unsafe { CUR_CHUNK_ID.load(Ordering::Relaxed) };
        let exti = unsafe { &*EXTI::ptr() };
        let syscfg = unsafe { &*SYSCFG::ptr() };

        unsafe { syscfg.exticr3.modify(|_, w| w.exti8().bits(6)) };

        exti.rtsr1.modify(|_, w| w.tr8().set_bit());
        exti.ftsr1.modify(|_, w| w.tr8().clear_bit());

        exti.cpuimr1.modify(|_, w| w.mr8().set_bit());

        exti.cpupr1.write(|w| w.pr8().set_bit());

        unsafe { cortex_m::peripheral::NVIC::unmask(Interrupt::EXTI9_5) };

        Self {
            prod,
            last_chunk: chunk_id,
            last_byte: 0,
        }
    }

    pub fn handle_nss(&mut self) {
        let exti = unsafe { &*EXTI::ptr() };

        if exti.cpupr1.read().pr8().bit_is_set() {
            exti.cpupr1.write(|w| w.pr8().set_bit());

            self.update_txn_queue();
        }
    }

    fn update_txn_queue(&mut self) {
        let bdma = unsafe { &*BDMA::ptr() };
        let end_byte =
            CHUNK_SIZE - bdma.ch[0].ndtr.read().ndt().bits() as usize;
        let end_chunk = unsafe { CUR_CHUNK_ID.load(Ordering::Relaxed) };

        let (end_txn_byte, end_txn_chunk) = if end_byte == 0 {
            (CHUNK_SIZE as u8, end_chunk - 1)
        } else {
            (end_byte as u8, end_chunk)
        };

        let txn = TransactionBorders {
            start_chunk: self.last_chunk,
            start_byte: self.last_byte,
            end_chunk: end_txn_chunk,
            end_byte: end_txn_byte,
        };

        let _ = self.prod.enqueue(txn);

        self.last_chunk = end_chunk;
        self.last_byte = end_byte as u8;
    }
}

pub struct KasliLink {
    bdma_queue: Consumer<'static, Chunk, CHUNK_QUEUE_SIZE>,
    txn_queue: Consumer<'static, TransactionBorders, CHUNK_QUEUE_SIZE>,
}

impl KasliLink {
    pub fn start(
        _mosi: Pin<'B', 5, Alternate<8>>,
        _nss: Pin<'G', 8, Alternate<5>>,
        _sck: Pin<'G', 13, Alternate<5>>,
        spi6_rec: Spi6,
        spi6_periph: SPI6,
        bdma_rec: Bdma,
        bdma_periph: BDMA,
        clocks: &CoreClocks,
    ) -> (Self, KasliLinkBdmaHandler, KasliLinkNssHandler) {
        let kasli_spi: Spi<SPI6, Enabled, u8> =
            spi6_periph.spi_unchecked(MODE_1, 20.MHz(), spi6_rec, clocks);
        let kasli_spi = kasli_spi.disable();

        let spi6 = unsafe { &*SPI6::ptr() };
        spi6.cr1.modify(|_, w| w.spe().disabled());
        spi6.cfg2.modify(|_, w| {
            w.master()
                .slave()
                .cpha()
                .set_bit()
                .cpol()
                .clear_bit()
                .lsbfrst()
                .msbfirst()
                .ioswp()
                .clear_bit()
                .ssoe()
                .disabled()
                .ssm()
                .disabled()
                .comm()
                .variant(COMM_A::Receiver)
        });

        spi6.cfg1
            .modify(|_, w| w.dsize().bits(7).rxdmaen().set_bit());
        spi6.cr1.modify(|_, w| w.spe().enabled());

        let (prod_chunks, cons_chunks) = unsafe { CHUNK_QUEUE.split() };
        let (prod_txns, cons_txns) = unsafe { TXN_QUEUE.split() };

        let bdma_handler = KasliLinkBdmaHandler::start(
            kasli_spi,
            bdma_rec,
            bdma_periph,
            prod_chunks,
        );
        let nss_handler = KasliLinkNssHandler::start(prod_txns);

        (
            Self {
                bdma_queue: cons_chunks,
                txn_queue: cons_txns,
            },
            bdma_handler,
            nss_handler,
        )
    }

    pub fn read<const N: usize>(
        &mut self,
        buf: &mut [u8; N],
    ) -> KasliLinkResult<usize> {
        let txn = self
            .txn_queue
            .dequeue()
            .ok_or(KasliLinkError::NoMsgAvailable)?;
        if txn.size() > N {
            return Err(KasliLinkError::BufferTooSmall);
        }

        let (mut chunk_id, mut chunk) = loop {
            if let Some((id, chunk)) = self.bdma_queue.peek() {
                if txn.start_chunk < *id {
                    return Err(KasliLinkError::DesyncBody);
                } else if txn.start_chunk == *id {
                    break (*id, chunk.as_slice());
                }

                let _ = self.bdma_queue.dequeue();
            } else if txn.start_chunk == get_current_chunk_id() {
                break (get_current_chunk_id(), &get_current_chunk()[..]);
            } else {
                while self.txn_queue.dequeue().is_some() {}
                return Err(KasliLinkError::DesyncTxn);
            }
        };

        let mut bytes_written = 0;
        let mut start_byte = txn.start_byte as usize;

        loop {
            if txn.end_chunk == chunk_id {
                let end_byte = txn.end_byte as usize;
                let portion_size = end_byte - start_byte;

                buf[bytes_written..bytes_written + portion_size]
                    .copy_from_slice(&chunk[start_byte..end_byte]);

                bytes_written += portion_size;

                break;
            } else {
                let portion_size = CHUNK_SIZE - start_byte;

                buf[bytes_written..bytes_written + portion_size]
                    .copy_from_slice(&chunk[start_byte..CHUNK_SIZE]);

                bytes_written += portion_size;
            }

            start_byte = 0;

            let _ = self.bdma_queue.dequeue();
            if let Some((id, c)) = self.bdma_queue.peek() {
                if chunk_id + 1 == *id {
                    chunk_id = *id;
                    chunk = c.as_slice();
                } else {
                    return Err(KasliLinkError::BodyPartiallyMissing);
                }
            } else {
                if chunk_id + 1 == get_current_chunk_id() {
                    chunk_id = get_current_chunk_id();
                    chunk = &get_current_chunk()[..];
                } else {
                    return Err(KasliLinkError::BodyPartiallyMissing);
                }
            }
        }

        Ok(bytes_written)
    }
}
