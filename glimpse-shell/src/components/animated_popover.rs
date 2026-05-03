use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use relm4::gtk::{self, glib, prelude::*};

const OPEN_CLASS: &str = "animated-popover--open";
const CLOSING_CLASS: &str = "animated-popover--closing";
const ROOT_CLASS: &str = "animated-popover";
const ANIMATION_DURATION: Duration = Duration::from_millis(160);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnimationState {
    Closed,
    Opening,
    Open,
    Closing,
}

pub struct AnimatedPopover {
    popover: gtk::Popover,
    state: Rc<Cell<AnimationState>>,
    generation: Rc<Cell<u64>>,
}

impl AnimatedPopover {
    pub fn new(popover: &gtk::Popover) -> Self {
        let state = Rc::new(Cell::new(AnimationState::Closed));

        popover.add_css_class(ROOT_CLASS);
        popover.connect_closed({
            let state = state.clone();
            move |popover| {
                state.set(AnimationState::Closed);
                popover.set_can_target(true);
                popover.remove_css_class(OPEN_CLASS);
                popover.remove_css_class(CLOSING_CLASS);
            }
        });

        Self {
            popover: popover.clone(),
            state,
            generation: Rc::new(Cell::new(0)),
        }
    }

    pub fn toggle(&mut self) {
        match self.state.get() {
            AnimationState::Closed | AnimationState::Closing => self.open(),
            AnimationState::Opening | AnimationState::Open => self.close(),
        }
    }

    pub fn open(&mut self) {
        self.bump_generation();
        self.state.set(AnimationState::Opening);
        self.popover.set_can_target(true);
        self.popover.remove_css_class(OPEN_CLASS);
        self.popover.remove_css_class(CLOSING_CLASS);
        self.popover.popup();

        let popover = self.popover.clone();
        let state = self.state.clone();
        let generation = self.generation.clone();
        let current_generation = generation.get();
        glib::idle_add_local_once(move || {
            if generation.get() != current_generation || state.get() != AnimationState::Opening {
                return;
            }

            popover.add_css_class(OPEN_CLASS);
            state.set(AnimationState::Open);
        });
    }

    pub fn close(&mut self) {
        if self.state.get() == AnimationState::Closed {
            return;
        }

        self.bump_generation();
        self.state.set(AnimationState::Closing);
        self.popover.set_can_target(false);
        self.popover.remove_css_class(OPEN_CLASS);
        self.popover.add_css_class(CLOSING_CLASS);

        let popover = self.popover.clone();
        let state = self.state.clone();
        let generation = self.generation.clone();
        let current_generation = generation.get();
        glib::timeout_add_local_once(ANIMATION_DURATION, move || {
            if generation.get() != current_generation || state.get() != AnimationState::Closing {
                return;
            }

            popover.popdown();
            popover.set_can_target(true);
            popover.remove_css_class(CLOSING_CLASS);
            state.set(AnimationState::Closed);
        });
    }

    fn bump_generation(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
    }
}
