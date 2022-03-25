use std::ptr::NonNull;
use std::ops::{Deref, Drop};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::mem::{self, MaybeUninit, ManuallyDrop};

#[derive(Debug)]
pub struct StaticArc<T> {
    inner: NonNull<StaticArcInner<T>>,
}

struct StaticArcInner<T> {
    counter: AtomicUsize,
    value: ManuallyDrop<T>,
}

unsafe impl<T> Send for StaticArc<T> {}

impl<T> StaticArc<T> {
    #[inline]
    pub fn new<const N: usize>(value: T) -> Option<[Self; N]> {
        Self::new_recover(value).ok()
    }

    pub fn new_recover<const N: usize>(value: T) -> Result<[Self; N], T> {
        if N < 1 {
            return Err(value);
        }

        let boxed = Box::new(StaticArcInner {
            value: ManuallyDrop::new(value),
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

    #[inline]
    fn arc(&self) -> &mut StaticArcInner<T> {
        // SAFETY: this `StaticArc` has already been initialized
        unsafe { &mut *self.inner.as_ptr() }
    }

    #[inline]
    pub fn live(&self) -> usize {
        self.arc().counter.load(Ordering::SeqCst)
    }

    #[inline]
    pub fn try_as_ref_mut(&self) -> Option<&mut T> {
        if self.live() == 1 {
            Some(&mut self.arc().value)
        } else {
            None
        }
    }

    #[inline]
    pub fn try_into_inner(self) -> Option<T> {
        self.try_into_inner_recover().ok()
    }

    pub fn try_into_inner_recover(self) -> Result<T, Self> {
        match self.try_as_ref_mut() {
            Some(value) => {
                // SAFETY: a single instance remains, so
                // we can reclaim the allocated value
                let value = unsafe {
                    std::ptr::read(value as *const _)
                };

                // SAFETY: manually drop `Box`, since we want to
                // keep the inner value
                let _ = unsafe { Box::from_raw(self.inner.as_ptr()) };
                mem::forget(self);

                Ok(value)
            },
            None => Err(self),
        }
    }
}

impl<T> Deref for StaticArc<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.arc().value
    }
}

impl<T> Drop for StaticArc<T> {
    fn drop(&mut self) {
        if self.arc().counter.fetch_sub(1, Ordering::SeqCst) == 1 {
            // SAFETY: counter value reached 0, therefore
            // no more `StaticArc` instances are alive
            unsafe {
                // drop value
                ManuallyDrop::drop(&mut self.inner.as_mut().value);

                // drop box allocation
                let _ = Box::from_raw(self.inner.as_ptr());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn test_ref_mut() {
        let [p1, p2, p3, p4] = StaticArc::new(Mutex::new(1234)).unwrap();
        std::thread::spawn(move || {
            drop((p2, p3));
        });
        assert_eq!(*p4.lock().unwrap(), 1234);
        drop(p4);
        loop {
            match p1.try_as_ref_mut() {
                Some(p) => {
                    *p.lock().unwrap() = 420;
                    break;
                },
                _ => (),
            }
        }
        let x = p1.try_into_inner().unwrap();
        assert_eq!(*x.lock().unwrap(), 420);
    }
}
