use crate::net::miniconf::{TreeDeserialize, TreeKey, TreeSerialize};
use heapless::String;

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
            update = do_update;
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
                    true,
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
    str: String<STR_SIZE>,
}

impl KasliInterfaceStateText {
    fn new() -> Self {
        Self { str: String::new() }
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
        let start = *idx;

        while *idx < msg.len() {
            if msg[*idx] == b'\n' {
                let s = match core::str::from_utf8(&msg[start..*idx]) {
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
                if let Err(_e) = self.str.push_str(s) {
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

                let mut parts = self.str.splitn(2, ' ');
                let Some(path) = parts.next() else {
                    log::error!(
                        "KasliInterface: failed to split string: {}",
                        self.str
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
                        self.str
                    );
                    return (
                        KasliInterfaceStateMachine::SearchingForPreamble(
                            KasliInterfaceStatePreamble::new(),
                        ),
                        false,
                    );
                };
                let flavor = postcard::de_flavors::Slice::new(value.as_bytes());
                if let Err(e) = miniconf::postcard::set_by_key(
                    settings,
                    path.split('/').filter(|s| !s.is_empty()),
                    flavor,
                ) {
                    log::error!("KasliInterface: failed to update config, error: {}, string: {}", e, self.str);
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

            *idx += 1;
        }

        let s = match core::str::from_utf8(&msg[start..msg.len()]) {
            Ok(x) => x,
            Err(e) => {
                log::error!(
                    "KasliInterface: failed to verify utf-8 string, error: {}",
                    e
                );
                return (
                    KasliInterfaceStateMachine::SearchingForPreamble(
                        KasliInterfaceStatePreamble::new(),
                    ),
                    false,
                );
            }
        };

        let s = match self.str.push_str(s) {
            Ok(x) => x,
            Err(e) => {
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
        };

        (KasliInterfaceStateMachine::CollectingText(self), false)
    }
}
