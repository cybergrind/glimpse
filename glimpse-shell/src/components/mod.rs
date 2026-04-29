pub mod action_menu;
pub mod animated_popover;
pub mod collapsible_section;
pub mod device_list;
pub mod device_status;
pub mod hero;
pub mod key_value_grid;
pub mod popover_shell;

#[cfg(test)]
pub(crate) mod test_support {
    use std::{
        sync::{Mutex, OnceLock},
        thread::ThreadId,
    };

    use relm4::gtk;

    static GTK_INIT_LOCK: Mutex<()> = Mutex::new(());
    static GTK_TEST_THREAD: OnceLock<ThreadId> = OnceLock::new();

    pub fn gtk_available_on_this_thread() -> bool {
        let Ok(_guard) = GTK_INIT_LOCK.lock() else {
            return false;
        };

        if gtk::is_initialized() {
            return GTK_TEST_THREAD
                .get()
                .is_some_and(|thread| *thread == std::thread::current().id());
        }

        if gtk::init().is_err() {
            return false;
        }

        let _ = GTK_TEST_THREAD.set(std::thread::current().id());
        true
    }
}
