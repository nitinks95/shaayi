//! Platform-neutral pointer detach/reattach state machine.

pub trait PointerBackend {
    fn detach(&mut self) -> Result<(), String>;
    fn reattach(&mut self) -> Result<(), String>;
}

pub struct PointerLock<B: PointerBackend> {
    backend: B,
    detached: bool,
}

impl<B: PointerBackend> PointerLock<B> {
    pub fn new(backend: B) -> Self {
        Self { backend, detached: false }
    }

    pub fn enter_edit(&mut self) -> Result<(), String> {
        if self.detached {
            return Ok(());
        }
        self.backend.detach()?;
        self.detached = true;
        Ok(())
    }

    pub fn exit_edit(&mut self) -> Result<(), String> {
        if !self.detached {
            return Ok(());
        }
        self.backend.reattach()?;
        self.detached = false;
        Ok(())
    }

    pub fn is_detached(&self) -> bool {
        self.detached
    }

    /// Always reattaches, regardless of tracked state (recovers from a crash mid-edit).
    pub fn startup_recover(&mut self) -> Result<(), String> {
        self.backend.reattach()?;
        self.detached = false;
        Ok(())
    }
}
