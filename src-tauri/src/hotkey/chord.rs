//! The chord-resolution state machine (PROMPT.md §6).
//!
//! Pure and clock-agnostic on purpose: timing is injected as an
//! `ArmTimerFired` event rather than read from a real clock, so the whole
//! thing is testable without sleeping. The async driver in `hotkey::run`
//! owns the real 150 ms timer and feeds this machine.

/// The chord-resolution window: on `RightCommand` key-down we don't yet
/// know whether the user wants Dictate or Command mode, since Command mode
/// is Right Command *plus* Right Option. We wait this long before deciding.
pub const ARM_WINDOW_MS: u64 = 150;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dictate,
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Arming,
    Recording(Mode),
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordEvent {
    RightCommandDown { at_ms: u64 },
    RightCommandUp { at_ms: u64 },
    RightOptionDown,
    RightOptionUp,
    Escape,
    /// Fed in by the driver once the real 150 ms timer elapses.
    ArmTimerFired { at_ms: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordOutcome {
    /// Nothing the caller needs to act on.
    None,
    /// Entered Arming — start buffering audio immediately (hazard #2: do
    /// NOT wait for the timer, or the first syllable gets clipped).
    ArmStarted,
    /// The 150 ms window elapsed; mode is now known — show the pill.
    ModeResolved(Mode),
    /// Esc was pressed mid-recording — discard, type nothing.
    Cancelled,
    /// Released during Arming, before the timer fired — treat as a no-op tap.
    AccidentalTap,
    /// A session (recording or cancelled) just ended on key-up — back to Idle.
    Idle,
}

pub struct ChordMachine {
    phase: Phase,
    right_option_held: bool,
}

impl ChordMachine {
    pub fn new() -> Self {
        Self {
            phase: Phase::Idle,
            right_option_held: false,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn on_event(&mut self, event: ChordEvent) -> ChordOutcome {
        match event {
            ChordEvent::RightOptionDown => {
                self.right_option_held = true;
                ChordOutcome::None
            }
            ChordEvent::RightOptionUp => {
                self.right_option_held = false;
                ChordOutcome::None
            }
            ChordEvent::RightCommandDown { .. } => {
                if self.phase == Phase::Idle {
                    self.phase = Phase::Arming;
                    ChordOutcome::ArmStarted
                } else {
                    ChordOutcome::None
                }
            }
            ChordEvent::ArmTimerFired { .. } => {
                if self.phase == Phase::Arming {
                    let mode = if self.right_option_held {
                        Mode::Command
                    } else {
                        Mode::Dictate
                    };
                    self.phase = Phase::Recording(mode);
                    ChordOutcome::ModeResolved(mode)
                } else {
                    ChordOutcome::None
                }
            }
            ChordEvent::Escape => {
                if matches!(self.phase, Phase::Recording(_)) {
                    self.phase = Phase::Cancelled;
                    ChordOutcome::Cancelled
                } else {
                    ChordOutcome::None
                }
            }
            ChordEvent::RightCommandUp { .. } => {
                let outcome = match self.phase {
                    Phase::Arming => ChordOutcome::AccidentalTap,
                    Phase::Recording(_) | Phase::Cancelled => ChordOutcome::Idle,
                    Phase::Idle => ChordOutcome::None,
                };
                self.phase = Phase::Idle;
                outcome
            }
        }
    }
}

impl Default for ChordMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_right_command_arms_then_resolves_to_dictate() {
        let mut m = ChordMachine::new();
        assert_eq!(
            m.on_event(ChordEvent::RightCommandDown { at_ms: 0 }),
            ChordOutcome::ArmStarted
        );
        assert_eq!(m.phase(), Phase::Arming);
        assert_eq!(
            m.on_event(ChordEvent::ArmTimerFired { at_ms: 150 }),
            ChordOutcome::ModeResolved(Mode::Dictate)
        );
        assert_eq!(m.phase(), Phase::Recording(Mode::Dictate));
    }

    #[test]
    fn right_option_held_during_arming_resolves_to_command_mode() {
        let mut m = ChordMachine::new();
        m.on_event(ChordEvent::RightCommandDown { at_ms: 0 });
        m.on_event(ChordEvent::RightOptionDown);
        assert_eq!(
            m.on_event(ChordEvent::ArmTimerFired { at_ms: 150 }),
            ChordOutcome::ModeResolved(Mode::Command)
        );
        assert_eq!(m.phase(), Phase::Recording(Mode::Command));
    }

    #[test]
    fn right_option_pressed_after_resolution_does_not_change_mode() {
        let mut m = ChordMachine::new();
        m.on_event(ChordEvent::RightCommandDown { at_ms: 0 });
        assert_eq!(
            m.on_event(ChordEvent::ArmTimerFired { at_ms: 150 }),
            ChordOutcome::ModeResolved(Mode::Dictate)
        );
        m.on_event(ChordEvent::RightOptionDown);
        assert_eq!(m.phase(), Phase::Recording(Mode::Dictate));
    }

    #[test]
    fn releasing_before_the_timer_fires_is_an_accidental_tap() {
        let mut m = ChordMachine::new();
        m.on_event(ChordEvent::RightCommandDown { at_ms: 0 });
        assert_eq!(
            m.on_event(ChordEvent::RightCommandUp { at_ms: 80 }),
            ChordOutcome::AccidentalTap
        );
        assert_eq!(m.phase(), Phase::Idle);
    }

    #[test]
    fn escape_during_recording_cancels_and_release_returns_to_idle_silently() {
        let mut m = ChordMachine::new();
        m.on_event(ChordEvent::RightCommandDown { at_ms: 0 });
        m.on_event(ChordEvent::ArmTimerFired { at_ms: 150 });
        assert_eq!(m.on_event(ChordEvent::Escape), ChordOutcome::Cancelled);
        assert_eq!(m.phase(), Phase::Cancelled);
        assert_eq!(
            m.on_event(ChordEvent::RightCommandUp { at_ms: 400 }),
            ChordOutcome::Idle
        );
        assert_eq!(m.phase(), Phase::Idle);
    }

    #[test]
    fn escape_outside_a_recording_session_is_a_no_op() {
        let mut m = ChordMachine::new();
        assert_eq!(m.on_event(ChordEvent::Escape), ChordOutcome::None);
        assert_eq!(m.phase(), Phase::Idle);
    }

    #[test]
    fn command_mode_recording_also_cancels_on_escape() {
        let mut m = ChordMachine::new();
        m.on_event(ChordEvent::RightCommandDown { at_ms: 0 });
        m.on_event(ChordEvent::RightOptionDown);
        m.on_event(ChordEvent::ArmTimerFired { at_ms: 150 });
        assert_eq!(m.on_event(ChordEvent::Escape), ChordOutcome::Cancelled);
    }
}
