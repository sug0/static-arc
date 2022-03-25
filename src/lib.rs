use std::ptr::NonNull;
use std::ops::{Deref, Drop};
use std::mem::{MaybeUninit, ManuallyDrop};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
pub struct StaticArc<T> {
    inner: NonNull<StaticArcInner<T>>,
}

struct StaticArcInner<T> {
    counter: AtomicUsize,
    value: T,
}

unsafe impl<T> Send for StaticArc<T> {}

impl<T> StaticArc<T> {
    pub fn new<const N: usize>(value: T) -> Result<[Self; N], T> {
        if N < 1 {
            return Err(value);
        }

        let boxed = Box::new(StaticArcInner {
            value,
            counter: AtomicUsize::new(N),
        });

        // SAFETY: the boxed value has a valid heap address
        let inner = unsafe { NonNull::new_unchecked(Box::into_raw(boxed)) };

        let mut array: MaybeUninit<[StaticArc<T>; N]> = MaybeUninit::uninit();

        // initialize array
        for i in 0..N {
            // SAFETY: the addr of `array` is not null,
            // and we are pointing to an index in `array`
            // when writing a value
            unsafe {
                array
                    .as_mut_ptr()
                    .cast::<StaticArc<T>>()
                    .add(i)
                    .write(StaticArc { inner })
            }
        }

        // SAFETY: we initialized `array`
        Ok(unsafe { array.assume_init() })
    }

    pub fn try_as_ref_mut(&self) -> Option<&mut T> {
        // SAFETY: this `StaticArc` has already been initialized
        let arc = unsafe { &mut *self.inner.as_ptr() };

        if arc.counter.load(Ordering::SeqCst) == 1 {
            Some(&mut arc.value)
        } else {
            None
        }
    }

    pub fn try_into_inner(self) -> Result<T, Self> {
        match self.try_as_ref_mut() {
            Some(value) => {
                // SAFETY: a single instance remains, so
                // we can reclaim the allocated value
                let value = unsafe {
                    std::ptr::read(value as *const _)
                };

                // manually drop, to make this process a bit more
                // efficient (no need to perform more atomic ops)
                let arc = ManuallyDrop::new(self);
                let _ = unsafe { Box::from_raw(arc.inner.as_ptr()) };

                Ok(value)
            },
            None => Err(self),
        }
    }
}

impl<T> Deref for StaticArc<T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: this `StaticArc` has already been initialized
        let arc = unsafe { &*self.inner.as_ptr() };

        &arc.value
    }
}

impl<T> Drop for StaticArc<T> {
    fn drop(&mut self) {
        // SAFETY: this `StaticArc` has already been initialized
        let arc = unsafe { &*self.inner.as_ptr() };

        if arc.counter.fetch_sub(1, Ordering::SeqCst) == 1 {
            // SAFETY: counter value reached 0, therefore
            // no more `StaticArc` instances are alive
            let _ = unsafe { Box::from_raw(self.inner.as_ptr()) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_ref_mut() {
        let [p1, p2, p3, p4] = StaticArc::new(1234).unwrap();
        thread::spawn(move || {
            drop((p2, p3));
        });
        thread::sleep(Duration::from_secs(1));
        assert_eq!(*p4, 1234);
        drop(p4);
        *p1.try_as_ref_mut().unwrap() = 420;
        let x = p1.try_into_inner().unwrap();
        assert_eq!(x, 420);
    }
}
