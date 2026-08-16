//! Downmix + resample raw device-rate audio to the 16 kHz mono f32 all
//! ASR engines expect (PROMPT.md §3, §7).

/// Interleaved multi-channel samples -> mono by averaging channels.
pub fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    let channels = channels.max(1) as usize;
    if channels == 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

/// Linear-interpolation resample. Not broadcast quality, but sufficient
/// for ASR input and dependency-free.
pub fn resample_linear(input: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if from_hz == to_hz || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from_hz as f64 / to_hz as f64;
    let out_len = ((input.len() as f64) / ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;
        let a = input[idx.min(input.len() - 1)];
        let b = input[(idx + 1).min(input.len() - 1)];
        out.push(a + (b - a) * frac);
    }
    out
}

/// Raw captured audio -> what every `AsrEngine` expects: 16 kHz mono f32.
pub fn to_asr_input(interleaved: &[f32], channels: u16, source_hz: u32) -> Vec<f32> {
    let mono = downmix_to_mono(interleaved, channels);
    resample_linear(&mono, source_hz, 16_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_stereo_averages_channels() {
        let stereo = [1.0, 3.0, 2.0, 4.0]; // frames: (1,3) (2,4)
        assert_eq!(downmix_to_mono(&stereo, 2), vec![2.0, 3.0]);
    }

    #[test]
    fn downmix_mono_is_a_no_op() {
        let mono = [1.0, 2.0, 3.0];
        assert_eq!(downmix_to_mono(&mono, 1), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn resample_same_rate_is_a_no_op() {
        let input = [1.0, 2.0, 3.0];
        assert_eq!(resample_linear(&input, 16_000, 16_000), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn resample_downsamples_to_expected_length() {
        // 48 kHz -> 16 kHz is a 3x downsample.
        let input: Vec<f32> = (0..4800).map(|i| i as f32).collect();
        let out = resample_linear(&input, 48_000, 16_000);
        assert_eq!(out.len(), 1600);
    }

    #[test]
    fn resample_preserves_a_constant_signal() {
        let input = vec![0.5_f32; 1000];
        let out = resample_linear(&input, 44_100, 16_000);
        assert!(out.iter().all(|&s| (s - 0.5).abs() < 1e-6));
    }
}
