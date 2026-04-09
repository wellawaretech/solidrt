use rquickjs::JsLifetime;
use std::cell::Cell;
use std::rc::Rc;

/// Tracks pending async operations that should keep the engine alive.
#[derive(Clone, JsLifetime)]
pub(crate) struct PendingOps {
    count: Rc<Cell<u32>>,
    notify: Rc<tokio::sync::Notify>,
}

impl PendingOps {
    pub(crate) fn new() -> Self {
        Self {
            count: Rc::new(Cell::new(0)),
            notify: Rc::new(tokio::sync::Notify::new()),
        }
    }

    pub(crate) fn hold(&self) {
        self.count.set(self.count.get() + 1);
    }

    pub(crate) fn release(&self) {
        let n = self.count.get() - 1;
        self.count.set(n);
        if n == 0 {
            self.notify.notify_waiters();
        }
    }

    pub(crate) async fn wait_idle(&self) {
        loop {
            if self.count.get() == 0 {
                return;
            }
            self.notify.notified().await;
        }
    }
}
