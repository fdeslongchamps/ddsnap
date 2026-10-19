//! Blocking work (psd-tools helper, DDS encoding) runs on a thread; results come back to the
//! GTK thread by polling a channel, so the window never freezes.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

use ddsnap::core::helper::{Helper, LayerImage, Result};
use ddsnap::core::layers::LayerTree;
use gtk::glib;

pub enum Msg {
    /// A French label for the progress bar and the log.
    Step(String),
    Fraction(f64),
    /// A layer extracted for the preview; `None` when the helper refused it.
    Layer(usize, Option<LayerImage>),
}

/// The helper and the PSD it has open.
pub struct Session {
    pub helper: Helper,
    psd: Option<PathBuf>,
}

impl Session {
    pub fn spawn() -> Result<Session> {
        Ok(Session {
            helper: Helper::spawn()?,
            psd: None,
        })
    }

    pub fn open(&mut self, psd: &Path) -> Result<LayerTree> {
        self.psd = None;
        let tree = self.helper.open(psd)?;
        self.psd = Some(psd.into());
        Ok(tree)
    }

    /// Opens `psd` again when the helper was restarted since.
    pub fn ensure(&mut self, psd: &Path) -> Result<()> {
        if self.psd.as_deref() != Some(psd) {
            self.open(psd)?;
        }
        Ok(())
    }
}

/// Runs `work` on a thread; `on_msg` and `done` run on the GTK thread.
pub fn background<T: Send + 'static>(
    work: impl FnOnce(&Sender<Msg>) -> T + Send + 'static,
    on_msg: impl Fn(Msg) + 'static,
    done: impl FnOnce(T) + 'static,
) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || work(&tx));
    let mut pending = Some((handle, done));
    glib::timeout_add_local(Duration::from_millis(50), move || {
        rx.try_iter().for_each(&on_msg);
        if !pending.as_ref().is_some_and(|(h, _)| h.is_finished()) {
            return glib::ControlFlow::Continue;
        }
        let (handle, done) = pending.take().expect("checked above");
        rx.try_iter().for_each(&on_msg); // sent between the drain above and the end
        match handle.join() {
            Ok(value) => done(value),
            Err(panic) => std::panic::resume_unwind(panic),
        }
        glib::ControlFlow::Break
    });
}
