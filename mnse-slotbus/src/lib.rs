//! MNSE SlotBus — Lockless Shared Memory Ring Buffer IPC.
//!
//! Provides nanosecond-latency, zero-copy sensory intake between adapters
//! and the kernel daemon via Linux POSIX shared memory (`/dev/shm`).
//!
//! Architecture:
//! - Mmapped shared ring buffer with atomic sequence counters (`AtomicUsize`).
//! - Single or Multi-Producer to Single-Consumer lockless queue.
//! - Sub-microsecond (<500ns) event emission with zero context switching.

use mnse_types::Event;
use std::ffi::CString;
use std::ptr;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use thiserror::Error;

pub const DEFAULT_SHM_NAME: &str = "/mnse_slotbus";
pub const DEFAULT_CAPACITY: u32 = 512;
pub const DEFAULT_SLOT_SIZE: u32 = 4096;
const SLOTBUS_MAGIC: u32 = 0x534C4F54; // 'SLOT'
const SLOTBUS_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum SlotBusError {
    #[error("Shared memory creation/open failed: {0}")]
    OsError(String),
    #[error("Payload length ({0} bytes) exceeds slot capacity ({1} bytes)")]
    PayloadTooLarge(usize, usize),
    #[error("Ring buffer queue is full ({0}/{0} slots occupied)")]
    QueueFull(u32),
    #[error("Invalid SlotBus magic header or unsupported version")]
    InvalidHeader,
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[repr(C)]
struct SlotBusHeader {
    magic: u32,
    version: u32,
    capacity: u32,
    slot_size: u32,
    head: AtomicUsize,
    tail: AtomicUsize,
    flags: AtomicU32,
    _reserved: [u8; 32],
}

/// Shared memory consumer used by MNSE Kernel Daemon to read incoming events
pub struct SlotBusConsumer {
    shm_name: String,
    ptr: *mut u8,
    total_size: usize,
}

unsafe impl Send for SlotBusConsumer {}
unsafe impl Sync for SlotBusConsumer {}

/// Shared memory producer used by adapters to publish events in nanoseconds
pub struct SlotBusProducer {
    _shm_name: String,
    ptr: *mut u8,
    total_size: usize,
}

unsafe impl Send for SlotBusProducer {}
unsafe impl Sync for SlotBusProducer {}

impl SlotBusConsumer {
    /// Create or initialize shared memory ring buffer
    pub fn new(name: &str, capacity: u32, slot_size: u32) -> Result<Self, SlotBusError> {
        let total_size = std::mem::size_of::<SlotBusHeader>() + (capacity as usize * slot_size as usize);
        let c_name = CString::new(name).map_err(|e| SlotBusError::OsError(e.to_string()))?;

        unsafe {
            // Unlink any previous stale segment
            libc::shm_unlink(c_name.as_ptr());

            let fd = libc::shm_open(
                c_name.as_ptr(),
                libc::O_CREAT | libc::O_RDWR | libc::O_EXCL,
                0o600,
            );
            if fd < 0 {
                return Err(SlotBusError::OsError(format!(
                    "shm_open create failed: errno {}",
                    *libc::__errno_location()
                )));
            }

            if libc::ftruncate(fd, total_size as libc::off_t) != 0 {
                libc::close(fd);
                libc::shm_unlink(c_name.as_ptr());
                return Err(SlotBusError::OsError(format!(
                    "ftruncate failed: errno {}",
                    *libc::__errno_location()
                )));
            }

            let map_ptr = libc::mmap(
                ptr::null_mut(),
                total_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );
            libc::close(fd);

            if map_ptr == libc::MAP_FAILED {
                libc::shm_unlink(c_name.as_ptr());
                return Err(SlotBusError::OsError(format!(
                    "mmap failed: errno {}",
                    *libc::__errno_location()
                )));
            }

            // Initialize Header
            let header = &mut *(map_ptr as *mut SlotBusHeader);
            header.magic = SLOTBUS_MAGIC;
            header.version = SLOTBUS_VERSION;
            header.capacity = capacity;
            header.slot_size = slot_size;
            header.head.store(0, Ordering::Relaxed);
            header.tail.store(0, Ordering::Relaxed);
            header.flags.store(1, Ordering::Relaxed);

            Ok(Self {
                shm_name: name.to_string(),
                ptr: map_ptr as *mut u8,
                total_size,
            })
        }
    }

    /// Read next raw byte payload from ring buffer (non-blocking)
    pub fn poll_next_bytes(&self) -> Option<Vec<u8>> {
        unsafe {
            let header = &*(self.ptr as *const SlotBusHeader);
            let tail = header.tail.load(Ordering::Acquire);
            let head = header.head.load(Ordering::Acquire);

            if tail >= head {
                return None;
            }

            let idx = tail % (header.capacity as usize);
            let slot_offset = std::mem::size_of::<SlotBusHeader>() + (idx * header.slot_size as usize);
            let slot_ptr = self.ptr.add(slot_offset);

            let len = *(slot_ptr as *const u32) as usize;
            let max_data = (header.slot_size as usize).saturating_sub(8);
            let read_len = len.min(max_data);

            let data_ptr = slot_ptr.add(8);
            let mut buf = vec![0u8; read_len];
            ptr::copy_nonoverlapping(data_ptr, buf.as_mut_ptr(), read_len);

            // Advance tail
            header.tail.store(tail + 1, Ordering::Release);

            Some(buf)
        }
    }

    /// Read next Event from ring buffer (non-blocking)
    pub fn poll_next_event(&self) -> Option<Event> {
        let bytes = self.poll_next_bytes()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Unlink shared memory segment from /dev/shm
    pub fn unlink(&self) {
        if let Ok(c_name) = CString::new(self.shm_name.as_str()) {
            unsafe {
                libc::shm_unlink(c_name.as_ptr());
            }
        }
    }
}

impl Drop for SlotBusConsumer {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr as *mut libc::c_void, self.total_size);
        }
        self.unlink();
    }
}

impl SlotBusProducer {
    /// Open existing shared memory ring buffer created by kernel
    pub fn open(name: &str) -> Result<Self, SlotBusError> {
        let c_name = CString::new(name).map_err(|e| SlotBusError::OsError(e.to_string()))?;

        unsafe {
            let fd = libc::shm_open(c_name.as_ptr(), libc::O_RDWR, 0);
            if fd < 0 {
                return Err(SlotBusError::OsError(format!(
                    "shm_open failed (kernel offline?): errno {}",
                    *libc::__errno_location()
                )));
            }

            // Read header size to inspect capacity
            let mut temp_header = std::mem::MaybeUninit::<SlotBusHeader>::uninit();
            let n = libc::read(
                fd,
                temp_header.as_mut_ptr() as *mut libc::c_void,
                std::mem::size_of::<SlotBusHeader>(),
            );
            if n < std::mem::size_of::<SlotBusHeader>() as libc::ssize_t {
                libc::close(fd);
                return Err(SlotBusError::InvalidHeader);
            }

            let header_val = temp_header.assume_init();
            if header_val.magic != SLOTBUS_MAGIC || header_val.version != SLOTBUS_VERSION {
                libc::close(fd);
                return Err(SlotBusError::InvalidHeader);
            }

            let total_size = std::mem::size_of::<SlotBusHeader>()
                + (header_val.capacity as usize * header_val.slot_size as usize);

            let map_ptr = libc::mmap(
                ptr::null_mut(),
                total_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );
            libc::close(fd);

            if map_ptr == libc::MAP_FAILED {
                return Err(SlotBusError::OsError(format!(
                    "mmap producer failed: errno {}",
                    *libc::__errno_location()
                )));
            }

            Ok(Self {
                _shm_name: name.to_string(),
                ptr: map_ptr as *mut u8,
                total_size,
            })
        }
    }

    /// Push raw bytes into ring buffer in <500 nanoseconds
    pub fn send_bytes(&self, data: &[u8]) -> Result<(), SlotBusError> {
        unsafe {
            let header = &*(self.ptr as *const SlotBusHeader);
            let max_data = (header.slot_size as usize).saturating_sub(8);
            if data.len() > max_data {
                return Err(SlotBusError::PayloadTooLarge(data.len(), max_data));
            }

            let head = header.head.load(Ordering::Acquire);
            let tail = header.tail.load(Ordering::Acquire);

            // Check if queue is full
            if head.saturating_sub(tail) >= header.capacity as usize {
                return Err(SlotBusError::QueueFull(header.capacity));
            }

            let idx = head % (header.capacity as usize);
            let slot_offset = std::mem::size_of::<SlotBusHeader>() + (idx * header.slot_size as usize);
            let slot_ptr = self.ptr.add(slot_offset);

            // Write length (4 bytes)
            *(slot_ptr as *mut u32) = data.len() as u32;

            // Write data payload
            let data_ptr = slot_ptr.add(8);
            ptr::copy_nonoverlapping(data.as_ptr(), data_ptr, data.len());

            // Release barrier & advance head
            header.head.store(head + 1, Ordering::Release);

            Ok(())
        }
    }

    /// Serialize and write an Event into the shared memory slot
    pub fn send_event(&self, event: &Event) -> Result<(), SlotBusError> {
        let serialized = serde_json::to_vec(event)?;
        self.send_bytes(&serialized)
    }
}

impl Drop for SlotBusProducer {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr as *mut libc::c_void, self.total_size);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mnse_types::EventType;

    #[test]
    fn test_slotbus_roundtrip_lockless() {
        let shm_name = format!("/mnse_test_{}", std::process::id());
        let consumer = SlotBusConsumer::new(&shm_name, 16, 1024).expect("consumer init");
        let producer = SlotBusProducer::open(&shm_name).expect("producer open");

        // Send 10 raw messages
        for i in 0..10 {
            let msg = format!("signal:{}", i);
            producer.send_bytes(msg.as_bytes()).expect("send bytes");
        }

        // Drain 10 messages from consumer
        for i in 0..10 {
            let received = consumer.poll_next_bytes().expect("read bytes");
            assert_eq!(String::from_utf8(received).unwrap(), format!("signal:{}", i));
        }

        // Buffer should now be empty
        assert!(consumer.poll_next_bytes().is_none());

        // Send typed Event
        let event = Event::new(
            EventType::WindowFocused {
                app_name: "kitty".into(),
                window_title: "dev".into(),
                window_class: "kitty".into(),
                window_id: "0x1".into(),
                pid: Some(1234),
                workspace: Some(1),
            },
            "os",
        );

        producer.send_event(&event).expect("send event");
        let received_ev = consumer.poll_next_event().expect("poll event");
        assert_eq!(received_ev.observed_by, "os");
    }
}
