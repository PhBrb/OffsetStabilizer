use crate::net::miniconf::{TreeDeserialize, TreeKey, TreeSerialize};
use heapless::Vec;

const STR_SIZE: usize = 128;

enum KasliInterfaceStateMachine {
    SearchingForPreamble(KasliInterfaceStatePreamble),
    CollectingText(KasliInterfaceStateText),
}

impl Default for KasliInterfaceStateMachine {
    fn default() -> Self {
        KasliInterfaceStateMachine::SearchingForPreamble(
            KasliInterfaceStatePreamble::new(),
        )
    }
}

impl KasliInterfaceStateMachine {
    fn progress<C>(
        self,
        msg: &[u8],
        idx: &mut usize,
        settings: &mut C,
    ) -> (Option<Self>, bool)
    where
        C: TreeKey + TreeSerialize,
        for<'de> C: TreeDeserialize<'de>,
    {
        let (new_state, should_update) = match self {
            Self::SearchingForPreamble(inner) => inner.progress(msg, idx),
            Self::CollectingText(inner) => inner.progress(msg, idx, settings),
        };

        (Some(new_state), should_update)
    }
}

pub struct KasliInterface {
    state: Option<KasliInterfaceStateMachine>,
}

impl KasliInterface {
    pub fn new() -> Self {
        Self {
            state: Some(KasliInterfaceStateMachine::default()),
        }
    }

    pub fn update<C>(&mut self, msg: &[u8], settings: &mut C) -> bool
    where
        C: TreeKey + TreeSerialize,
        for<'de> C: TreeDeserialize<'de>,
    {
        let mut processed = 0;
        let mut update = false;
        let mut state = self.state.take().unwrap();

        loop {
            let (new_state, do_update) =
                state.progress(msg, &mut processed, settings);
            state = new_state.unwrap();
            update |= do_update;
            if processed >= msg.len() {
                break;
            }
        }
        self.state.replace(state);

        update
    }
}

struct KasliInterfaceStatePreamble {
    preamble_signs_collected: usize,
}

impl KasliInterfaceStatePreamble {
    const PREAMBLE_SIGN: u8 = 255;
    const PREAMBLE_LEN: usize = 4;

    fn new() -> Self {
        Self {
            preamble_signs_collected: 0,
        }
    }

    fn progress(
        mut self,
        msg: &[u8],
        idx: &mut usize,
    ) -> (KasliInterfaceStateMachine, bool) {
        while *idx < msg.len() {
            if msg[*idx] == Self::PREAMBLE_SIGN {
                self.preamble_signs_collected += 1;
            } else {
                self.preamble_signs_collected = 0;
            }

            *idx += 1;

            if self.preamble_signs_collected == Self::PREAMBLE_LEN {
                return (
                    KasliInterfaceStateMachine::CollectingText(
                        KasliInterfaceStateText::new(),
                    ),
                    false,
                );
            }
        }

        (
            KasliInterfaceStateMachine::SearchingForPreamble(self),
            false,
        )
    }
}

struct KasliInterfaceStateText {
    bytes: Vec<u8, STR_SIZE>,
    preamble_signs_collected: usize,
}

impl KasliInterfaceStateText {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            preamble_signs_collected: 0,
        }
    }

    fn progress<C>(
        mut self,
        msg: &[u8],
        idx: &mut usize,
        settings: &mut C,
    ) -> (KasliInterfaceStateMachine, bool)
    where
        C: TreeKey + TreeSerialize,
        for<'de> C: TreeDeserialize<'de>,
    {
        while *idx < msg.len() {
            let b = msg[*idx];

            if b == KasliInterfaceStatePreamble::PREAMBLE_SIGN {
                self.preamble_signs_collected += 1;
                *idx += 1;

                if self.preamble_signs_collected
                    == KasliInterfaceStatePreamble::PREAMBLE_LEN
                {
                    // Preamble inside text indicates we lost synchronization.
                    // Restart collection from the new preamble.
                    return (
                        KasliInterfaceStateMachine::CollectingText(
                            KasliInterfaceStateText::new(),
                        ),
                        false,
                    );
                }

                continue;
            }
            self.preamble_signs_collected = 0;

            if b == b'\n' {
                let msg_str = match core::str::from_utf8(self.bytes.as_slice()) {
                    Ok(s) => s,
                    Err(e) => {
                        log::error!("KasliInterface: failed to verify utf-8 string, error: {}", e);
                        return (
                            KasliInterfaceStateMachine::SearchingForPreamble(
                                KasliInterfaceStatePreamble::new(),
                            ),
                            false,
                        );
                    }
                };

                let mut parts = msg_str.splitn(2, ' ');
                let Some(path) = parts.next() else {
                    log::error!(
                        "KasliInterface: failed to split string: {}",
                        msg_str
                    );
                    return (
                        KasliInterfaceStateMachine::SearchingForPreamble(
                            KasliInterfaceStatePreamble::new(),
                        ),
                        false,
                    );
                };
                let Some(value) = parts.next() else {
                    log::error!(
                        "KasliInterface: failed to split string: {}",
                        msg_str
                    );
                    return (
                        KasliInterfaceStateMachine::SearchingForPreamble(
                            KasliInterfaceStatePreamble::new(),
                        ),
                        false,
                    );
                };
                if let Err(e) = miniconf::json::set_by_key(
                    settings,
                    path.split('/').filter(|s| !s.is_empty()),
                    value.trim_end_matches('\r').as_bytes(),
                ) {
                    log::error!("KasliInterface: failed to update config, error: {}, string: {}", e, msg_str);
                    return (
                        KasliInterfaceStateMachine::SearchingForPreamble(
                            KasliInterfaceStatePreamble::new(),
                        ),
                        false,
                    );
                }

                *idx += 1;

                return (
                    KasliInterfaceStateMachine::SearchingForPreamble(
                        KasliInterfaceStatePreamble::new(),
                    ),
                    true,
                );
            }

            if let Err(_e) = self.bytes.push(b) {
                log::error!(
                    "KasliInterface: failed to fit message into {} bytes",
                    STR_SIZE
                );

                return (
                    KasliInterfaceStateMachine::SearchingForPreamble(
                        KasliInterfaceStatePreamble::new(),
                    ),
                    false,
                );
            }

            *idx += 1;
        }

        (KasliInterfaceStateMachine::CollectingText(self), false)
    }
}
