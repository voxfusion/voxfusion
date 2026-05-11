use hound::{SampleFormat, WavSpec, WavWriter};

#[tauri::command]
pub fn process_audio_file(path: String) -> Result<Vec<u8>, String> {
    let target_sample_rate: u32 = 16000;
    let mut reader =
        hound::WavReader::open(&path).map_err(|e| format!("Failed to open WAV file: {}", e))?;
    let source_spec = reader.spec();
    let source_rate = source_spec.sample_rate;
    let source_channels = source_spec.channels as usize;

    let samples_f32: Vec<f32> = match source_spec.sample_format {
        SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| s.map_err(|e| format!("Failed to read sample: {}", e)))
            .collect::<Result<Vec<f32>, String>>()?,
        SampleFormat::Int => {
            let bits = source_spec.bits_per_sample;
            let max_val = (1u32 << (bits - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| {
                    s.map(|v| v as f32 / max_val)
                        .map_err(|e| format!("Failed to read sample: {}", e))
                })
                .collect::<Result<Vec<f32>, String>>()?
        }
    };

    let mono_samples: Vec<f32> = if source_channels > 1 {
        samples_f32
            .chunks(source_channels)
            .map(|frame| frame.iter().sum::<f32>() / source_channels as f32)
            .collect()
    } else {
        samples_f32
    };

    let resampled = if source_rate == target_sample_rate {
        mono_samples
    } else {
        let ratio = source_rate as f64 / target_sample_rate as f64;
        let output_len = (mono_samples.len() as f64 / ratio) as usize;
        let mut output = Vec::with_capacity(output_len);

        for i in 0..output_len {
            let src_pos = i as f64 * ratio;
            let src_idx = src_pos as usize;
            let frac = (src_pos - src_idx as f64) as f32;

            let sample = if src_idx + 1 < mono_samples.len() {
                mono_samples[src_idx] * (1.0 - frac) + mono_samples[src_idx + 1] * frac
            } else if src_idx < mono_samples.len() {
                mono_samples[src_idx]
            } else {
                0.0
            };
            output.push(sample);
        }
        output
    };

    let output_spec = WavSpec {
        channels: 1,
        sample_rate: target_sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };

    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut writer = WavWriter::new(&mut buffer, output_spec)
            .map_err(|e| format!("Failed to create WAV writer: {}", e))?;

        for &sample in &resampled {
            let clamped = sample.clamp(-1.0, 1.0);
            let val = (clamped * 32767.0) as i16;
            writer
                .write_sample(val)
                .map_err(|e| format!("Failed to write sample: {}", e))?;
        }
        writer
            .finalize()
            .map_err(|e| format!("Failed to finalize WAV: {}", e))?;
    }

    Ok(buffer.into_inner())
}
