//! Reusable in-memory browser context lifetime, independent of any platform WebView type.

/// Keep one temporary context alive after its last tab closes, until explicit
/// clearing or process exit. Cookie bootstrap must happen only once per context.
pub(crate) struct SessionContext<T> {
    resource: Option<T>,
    restored: bool,
}
impl<T> Default for SessionContext<T> {
    fn default() -> Self {
        Self {
            resource: None,
            restored: false,
        }
    }
}
impl<T: Clone> SessionContext<T> {
    /// A failed creation never caches a partial context or marks cookies restored.
    pub fn get_or_try_init<E>(&mut self, create: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
        if let Some(resource) = &self.resource {
            return Ok(resource.clone());
        }
        let resource = create()?;
        self.resource = Some(resource.clone());
        Ok(resource)
    }
    /// Replaying stale disk cookies into a live shared store can undo a logout.
    pub fn needs_restore(&self) -> bool {
        !self.restored
    }
    /// Mark only after native cookie restoration completes successfully.
    pub fn mark_restored(&mut self) {
        self.restored = true;
    }
    /// Forget both context and bootstrap state when the user clears site data.
    pub fn reset(&mut self) {
        self.resource = None;
        self.restored = false;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};
    #[test]
    fn new_and_reopened_tabs_share_one_context_until_clear() {
        let mut session = SessionContext::default();
        let creates = Cell::new(0);
        let create = || -> Result<_, ()> {
            creates.set(creates.get() + 1);
            Ok(Rc::new(creates.get()))
        };
        let first = session.get_or_try_init(create).unwrap();
        session.mark_restored();
        let second = session.get_or_try_init(create).unwrap();
        assert!(Rc::ptr_eq(&first, &second));
        assert!(!session.needs_restore());
        drop(first);
        drop(second);
        let reopened = session.get_or_try_init(create).unwrap();
        assert_eq!(*reopened, 1);
        session.reset();
        assert!(session.needs_restore());
        let cleared = session.get_or_try_init(create).unwrap();
        assert_eq!(*cleared, 2);
        assert!(!Rc::ptr_eq(&reopened, &cleared));
    }
    #[test]
    fn failed_creation_is_retried_and_a_new_process_starts_empty() {
        let mut session = SessionContext::default();
        assert!(session.get_or_try_init(|| Err::<u8, _>("failed")).is_err());
        assert!(session.needs_restore());
        assert_eq!(session.get_or_try_init(|| Ok::<_, ()>(7)).unwrap(), 7);
        session.mark_restored();
        assert!(SessionContext::<u8>::default().needs_restore());
    }
}
