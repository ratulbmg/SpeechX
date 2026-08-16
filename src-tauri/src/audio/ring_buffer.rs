//! Lock-free SPSC ring buffer moving samples from the real-time `cpal`
//! callback thread to whoever drains the recording, without a mutex on
//! the audio callback's hot path.

use ringbuf::{HeapCons, HeapProd, HeapRb};

pub type SampleProducer = HeapProd<f32>;
pub type SampleConsumer = HeapCons<f32>;

/// `capacity` is in samples (not seconds) — size it for the device's
/// actual rate/channel count.
pub fn new(capacity: usize) -> (SampleProducer, SampleConsumer) {
    use ringbuf::traits::Split;
    HeapRb::<f32>::new(capacity).split()
}
