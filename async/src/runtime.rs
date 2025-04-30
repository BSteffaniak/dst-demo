pub trait GenericRuntime {}

pub struct Builder {
    pub max_blocking_threads: Option<u16>,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_blocking_threads: None,
        }
    }

    pub fn max_blocking_threads<T: Into<Option<u16>>>(
        &mut self,
        max_blocking_threads: T,
    ) -> &mut Self {
        self.max_blocking_threads = max_blocking_threads.into();
        self
    }
}
