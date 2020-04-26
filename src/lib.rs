use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

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

// TODO: implement Drop
#[derive(Clone, Debug)]
struct Handle(*mut sioctl_hdl);

impl Handle {
    fn as_ptr(&self) -> *mut sioctl_hdl {
        self.0
    }
}

unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

pub struct Sioctl {
    handle: Handle,
    shared: Arc<Shared>,
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

        unsafe {
            // TODO: Clean-up, as this will leak Shared.
            let arc = Arc::clone(&shared);
            let ptr = Arc::into_raw(arc) as *mut _;
            sioctl_ondesc(handle.as_ptr(), Some(ondesc), ptr);
            sioctl_onval(handle.as_ptr(), Some(onval), ptr);
        };

        Self { handle, shared }
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

        let handle = self.handle.clone();
        let thread = thread::spawn(|| polling_thread(self.handle));

        Watcher { handle, thread }
    }
}

struct Inner {
    controls: HashMap<Address, Control>,
    callback: Option<Box<dyn Fn(&Control) + Send + Sync>>,
}

/// Shared between the Rust objects and the C callbacks.
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

pub struct Watcher {
    handle: Handle,
    thread: JoinHandle<()>,
}

fn polling_thread(handle: Handle) {
    unsafe {
        let nfds = sioctl_nfds(handle.as_ptr());
        let mut pollfds = Vec::with_capacity(nfds as usize);
        let nfds = sioctl_pollfd(handle.as_ptr(), pollfds.as_mut_ptr(), POLLIN as i32);
        pollfds.set_len(nfds as usize);

        loop {
            while poll(pollfds.as_mut_ptr(), nfds as u32, -1) < 0 {
                let err = errno();
                if err != EINTR {
                    panic!("sioctl err: {}", err);
                }
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

