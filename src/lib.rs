use std::collections::HashMap;
use std::ffi::CStr;
use std::mem;
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::os::unix::io::RawFd;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;

use libc::{poll, EINTR, POLLIN, SIGHUP};
use nix::errno::errno;
use sndio_sys::*;

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
struct Address(c_uint);

#[derive(Clone, Debug)]
pub struct Control {
    pub group: String,
    pub name: String,
    pub func: String,
    pub value: u8,
}

#[derive(Debug)]
struct Handle(*mut sioctl_hdl);

unsafe impl Send for Handle {}

impl Handle {
    fn as_ptr(&self) -> *mut sioctl_hdl {
        self.0
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            sioctl_close(self.0);
        }
    }
}

pub struct Sioctl {
    handle: Handle,
    shared: Arc<Shared>,
    shared_ptr: SharedPtr,
}

impl Sioctl {
    pub fn new() -> Self {
        let handle = unsafe { sioctl_open(SIO_DEVANY.as_ptr() as *const _, SIOCTL_READ, 0) };
        let handle = Handle(handle);

        let inner = Mutex::new(Inner {
            controls: HashMap::new(),
            callback: None,
        });
        let shared = Arc::new(Shared { inner });

        // We need a pointer to pass the pointer from our Arc<Shared> to C.
        // We wrap this raw pointer in SharedPtr() and store it, so that the
        // Arc<Shared> is eventually dropped when SharedPtr() goes out of
        // scope.
        let arc = Arc::clone(&shared);
        let ptr = Arc::into_raw(arc);
        let shared_ptr = SharedPtr(ptr);

        unsafe {
            // Casting *const Shared to *mut _ looks suspicious. This is
            // because sndio requires a mutable pointer. We'll never mutate
            // it (and neither will sndio), so this should(?) be defined.
            let ptr = ptr as *mut _;
            sioctl_ondesc(handle.as_ptr(), Some(ondesc), ptr);
            sioctl_onval(handle.as_ptr(), Some(onval), ptr);
        };

        Self {
            handle,
            shared,
            shared_ptr,
        }
    }

    pub fn controls(&self) -> Vec<Control> {
        let inner = self.shared.inner.lock().unwrap();
        inner.controls.values().cloned().collect()
    }

    pub fn watch<C>(self, callback: C) -> Watcher
    where
        C: Fn(&Control) + Send + Sync + 'static,
    {
        {
            let mut inner = self.shared.inner.lock().unwrap();
            inner.callback = Some(Box::new(callback));
        }

        // We create a pipe so that we can wake up polling_thread() to tell it
        // to shutdown. Watcher will close(close_tx) when shutting down, which
        // will cause SIGHUP on close_rx.
        let (close_rx, close_tx) = nix::unistd::pipe().unwrap();

        let handle = self.handle;
        let thread_handle = thread::spawn(move || polling_thread(handle, close_rx));

        Watcher {
            shared_ptr: self.shared_ptr,
            thread_handle: Some(thread_handle),
            close_tx,
        }
    }
}

struct Inner {
    controls: HashMap<Address, Control>,
    callback: Option<Box<dyn Fn(&Control) + Send + Sync>>,
}

/// Shared between the Rust objects and the C callbacks.
/// Expects to be wrapped in an Arc to ensure appropriate lifetime.
struct Shared {
    inner: Mutex<Inner>,
}

impl Shared {
    fn on_parameter(&self, address: Address, control: Control) {
        let mut inner = self.inner.lock().unwrap();
        inner.controls.insert(address, control);
    }

    fn on_value(&self, address: Address, value: u8) {
        let mut inner = self.inner.lock().unwrap();
        inner
            .controls
            .entry(address)
            .and_modify(|control| control.value = value);

        // Intentionally call with the lock, so the callback can rely on
        // serial messages.
        if let Some(control) = inner.controls.get(&address) {
            if let Some(callback) = &inner.callback {
                (callback)(control)
            }
        }
    }
}

/// Wrapper around Arc<Shared>::into_raw() to ensure it is eventually Dropped.
/// In theory we should ensure this is dropped after the associated Handle.
/// In practise, we'll never get a callback as we don't call `sioctl_revents`
/// when we're dropping them, so it doesn't matter.
struct SharedPtr(*const Shared);

unsafe impl Send for SharedPtr {}

impl Drop for SharedPtr {
    fn drop(&mut self) {
        drop(unsafe { Arc::from_raw(self) });
    }
}

// (Allow dead code because we need to control the lifetime of these fields).
#[allow(dead_code)]
pub struct Watcher {
    shared_ptr: SharedPtr,
    close_tx: RawFd,
    thread_handle: Option<JoinHandle<()>>,
}

impl Watcher {
    pub fn join(&mut self) {
        if let Some(thread_handle) = mem::replace(&mut self.thread_handle, None) {
            // Close close_tx(), which will cause SIGHUP on close_rx in the
            // thread. The thread will then exit and we can wait for the
            // thread to join.
            nix::unistd::close(self.close_tx).unwrap();
            thread_handle.join().unwrap();
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.join();
    }
}

fn polling_thread(handle: Handle, close_rx: RawFd) {
    unsafe {
        let nfds = sioctl_nfds(handle.as_ptr()) as usize;
        let mut pollfds = Vec::with_capacity(nfds);
        let mut nfds = sioctl_pollfd(handle.as_ptr(), pollfds.as_mut_ptr(), POLLIN as i32) as usize;
        pollfds.set_len(nfds);

        // Place the fd that indicates shutdown last, so that it's ignored by
        // sioctl_revents() which will only look at first nfds.
        pollfds.push(pollfd {
            fd: close_rx,
            events: POLLIN,
            revents: 0,
        });
        let close_nfd = nfds;
        nfds += 1;

        loop {
            while poll(pollfds.as_mut_ptr(), nfds as u32, -1) < 0 {
                let err = errno();
                if err != EINTR {
                    panic!("sioctl err: {}", err);
                }
            }

            // Check if Watcher has asked us to exit via close_rx.
            if i32::from(pollfds[close_nfd].revents) & SIGHUP > 0 {
                nix::unistd::close(close_rx).unwrap();
                break;
            }

            let revents = sioctl_revents(handle.as_ptr(), pollfds.as_mut_ptr());
            if revents & SIGHUP > 0 {
                break;
            }
        }
    }
}

extern "C" fn onval(ptr: *mut c_void, addr: c_uint, value: c_uint) {
    unsafe {
        if let Some(shared) = (ptr as *const Shared).as_ref() {
            let address = Address(addr);
            let value = value as u8;
            shared.on_value(address, value);
        }
    }
}

extern "C" fn ondesc(ptr: *mut c_void, desc: *mut sioctl_desc, value: c_int) {
    unsafe {
        if let Some(desc) = desc.as_ref() {
            if let Some(shared) = (ptr as *const Shared).as_ref() {
                let address = Address(desc.addr);

                let name = parse_string(&desc.node0.name);
                let group = parse_string(&desc.group);
                let func = parse_string(&desc.func);
                let value = value as u8;
                let control = Control {
                    name,
                    group,
                    func,
                    value,
                };

                shared.on_parameter(address, control);
            }
        }
    }
}

unsafe fn parse_string(ptr: &[c_char]) -> String {
    CStr::from_ptr(ptr.as_ptr()).to_str().unwrap().to_owned()
}

