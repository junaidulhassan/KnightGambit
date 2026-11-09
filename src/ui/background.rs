use std::cell::RefCell;
use std::sync::mpsc;
use std::thread::JoinHandle;

use gtk::glib;

type Job = Box<dyn FnOnce() + Send + 'static>;

/// A single background thread that runs jobs in the order they were queued and
/// hands each result back to the GTK main loop. Used for database work so that
/// it never blocks drawing or engine handling during play.
pub struct Worker {
    sender: RefCell<Option<mpsc::Sender<Job>>>,
    thread: RefCell<Option<JoinHandle<()>>>,
}

impl Worker {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<Job>();
        let thread = std::thread::Builder::new()
            .name("knightgambit-history".to_owned())
            .spawn(move || {
                while let Ok(job) = receiver.recv() {
                    job();
                }
            })
            .ok();
        Self {
            sender: RefCell::new(Some(sender)),
            thread: RefCell::new(thread),
        }
    }

    /// Runs `work` on the worker thread, then `done` with its result on the main thread.
    pub fn run<T: Send + 'static>(&self, work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T) + 'static) {
        let (reply, answer) = async_channel::bounded::<T>(1);
        let job: Job = Box::new(move || {
            let _ = reply.send_blocking(work());
        });
        let queued = self.sender.borrow().as_ref().is_some_and(|sender| sender.send(job).is_ok());
        if queued {
            glib::spawn_future_local(async move {
                if let Ok(value) = answer.recv().await {
                    done(value);
                }
            });
        }
    }

    /// Finishes all queued jobs and stops the thread. Called when the window closes
    /// so that a game that has just ended is never lost.
    pub fn finish(&self) {
        self.sender.borrow_mut().take();
        if let Some(thread) = self.thread.borrow_mut().take() {
            let _ = thread.join();
        }
    }
}

impl Default for Worker {
    fn default() -> Self {
        Self::new()
    }
}
