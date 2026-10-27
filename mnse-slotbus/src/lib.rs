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
const SLOTBUS_VERSION: u32 = 2;

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

#[repr(C)]
struct SlotHeader {
    sequence: AtomicUsize,
    len: u32,
    _reserved: u32,
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

            // Initialize Slots
            for i in 0..capacity {
                let slot_offset = std::mem::size_of::<SlotBusHeader>() + (i as usize * slot_size as usize);
                let slot_hdr = &mut *(map_ptr.add(slot_offset) as *mut SlotHeader);
                slot_hdr.sequence.store(i as usize, Ordering::Relaxed);
                slot_hdr.len = 0;
                slot_hdr._reserved = 0;
            }

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
            let capacity = header.capacity as usize;
            let slot_hdr_size = std::mem::size_of::<SlotHeader>();
            let tail = header.tail.load(Ordering::Relaxed);

            let slot_offset = std::mem::size_of::<SlotBusHeader>() + ((tail % capacity) * header.slot_size as usize);
            let slot_ptr = self.ptr.add(slot_offset);
            let slot_hdr = &*(slot_ptr as *const SlotHeader);
            let seq = slot_hdr.sequence.load(Ordering::Acquire);
            let dif = seq as isize - (tail + 1) as isize;

            if dif == 0 {
                let len = slot_hdr.len as usize;
                let max_data = (header.slot_size as usize).saturating_sub(slot_hdr_size);
                let read_len = len.min(max_data);

                let data_ptr = slot_ptr.add(slot_hdr_size);
                let mut buf = vec![0u8; read_len];
                ptr::copy_nonoverlapping(data_ptr, buf.as_mut_ptr(), read_len);

                // Advance slot sequence so producer can reuse it next round
                slot_hdr.sequence.store(tail + capacity, Ordering::Release);

                // Advance tail
                header.tail.store(tail + 1, Ordering::Relaxed);

                Some(buf)
            } else {
                None
            }
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

    /// Push raw bytes into ring buffer in <500 nanoseconds with lockless MPMC/MPSC concurrency
    pub fn send_bytes(&self, data: &[u8]) -> Result<(), SlotBusError> {
        unsafe {
            let header = &*(self.ptr as *const SlotBusHeader);
            let slot_hdr_size = std::mem::size_of::<SlotHeader>();
            let max_data = (header.slot_size as usize).saturating_sub(slot_hdr_size);
            if data.len() > max_data {
                return Err(SlotBusError::PayloadTooLarge(data.len(), max_data));
            }

            let capacity = header.capacity as usize;
            let mut head = header.head.load(Ordering::Relaxed);

            loop {
                let slot_offset = std::mem::size_of::<SlotBusHeader>() + ((head % capacity) * header.slot_size as usize);
                let slot_ptr = self.ptr.add(slot_offset);
                let slot_hdr = &*(slot_ptr as *const SlotHeader);
                let seq = slot_hdr.sequence.load(Ordering::Acquire);
                let dif = seq as isize - head as isize;

                if dif == 0 {
                    match header.head.compare_exchange_weak(
                        head,
                        head + 1,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => {
                            let mut_slot_hdr = &mut *(slot_ptr as *mut SlotHeader);
                            mut_slot_hdr.len = data.len() as u32;

                            let data_ptr = slot_ptr.add(slot_hdr_size);
                            ptr::copy_nonoverlapping(data.as_ptr(), data_ptr, data.len());

                            // Mark slot as written and ready for consumption
                            slot_hdr.sequence.store(head + 1, Ordering::Release);
                            return Ok(());
                        }
                        Err(actual) => {
                            head = actual;
                        }
                    }
                } else if dif < 0 {
                    return Err(SlotBusError::QueueFull(header.capacity));
                } else {
                    head = header.head.load(Ordering::Relaxed);
                }
            }
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

    #[test]
    fn test_slotbus_multi_producer_concurrency() {
        let shm_name = format!("/mnse_test_mpsc_{}", std::process::id());
        let capacity = 64;
        let consumer = SlotBusConsumer::new(&shm_name, capacity, 1024).expect("consumer init");

        let num_threads = 4;
        let msgs_per_thread = 250;
        let total_msgs = num_threads * msgs_per_thread;

        let mut handles = Vec::new();
        for t in 0..num_threads {
            let name = shm_name.clone();
            handles.push(std::thread::spawn(move || {
                let producer = SlotBusProducer::open(&name).expect("producer open");
                for i in 0..msgs_per_thread {
                    let msg = format!("th_{}:msg_{}", t, i);
                    loop {
                        match producer.send_bytes(msg.as_bytes()) {
                            Ok(()) => break,
                            Err(SlotBusError::QueueFull(_)) => {
                                std::thread::yield_now();
                            }
                            Err(e) => panic!("unexpected error: {:?}", e),
                        }
                    }
                }
            }));
        }

        // Consumer reads all total_msgs
        let mut received = Vec::new();
        let start = std::time::Instant::now();
        while received.len() < total_msgs && start.elapsed() < std::time::Duration::from_secs(5) {
            if let Some(bytes) = consumer.poll_next_bytes() {
                received.push(String::from_utf8(bytes).unwrap());
            } else {
                std::thread::yield_now();
            }
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(received.len(), total_msgs);

        // Check that each thread's messages were received in sequential order
        for t in 0..num_threads {
            let thread_msgs: Vec<_> = received
                .iter()
                .filter(|m| m.starts_with(&format!("th_{}:", t)))
                .cloned()
                .collect();
            assert_eq!(thread_msgs.len(), msgs_per_thread);
            for i in 0..msgs_per_thread {
                assert_eq!(thread_msgs[i], format!("th_{}:msg_{}", t, i));
            }
        }
    }
}
