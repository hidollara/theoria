//! Backend-independent recording of rendered image sequences.

use std::{error::Error, fmt, io::Write, time::Duration};

use crate::{image::Srgba8Image, surface::SurfaceSize};

/// Records a known number of timed sRGBA8 frames into one animated PNG.
///
/// The recorder is independent of the system that renders each frame. Every
/// image carries the duration for which it remains visible.
pub struct AnimatedPngRecorder<W: Write> {
    output: AnimatedPngWriter<W>,
    recorded_frames: u32,
}

impl<W: Write> AnimatedPngRecorder<W> {
    /// Creates a single-play APNG recorder with a fixed extent and frame count.
    pub fn new(output: W, size: SurfaceSize, frame_count: u32) -> Result<Self, RecordingError> {
        Ok(Self {
            output: AnimatedPngWriter::new(output, size, frame_count)?,
            recorded_frames: 0,
        })
    }

    /// Returns the fixed extent of every recorded frame.
    pub const fn size(&self) -> SurfaceSize {
        self.output.size
    }

    /// Returns the number of frames required by this recording.
    pub const fn frame_count(&self) -> u32 {
        self.output.frame_count
    }

    /// Returns the number of frames recorded so far.
    pub const fn recorded_frames(&self) -> u32 {
        self.recorded_frames
    }

    /// Reports whether every required frame has been recorded.
    pub const fn is_complete(&self) -> bool {
        self.recorded_frames == self.output.frame_count
    }

    /// Records one rendered frame with the duration it remains visible.
    pub fn record(&mut self, image: Srgba8Image, duration: Duration) -> Result<(), RecordingError> {
        if self.is_complete() {
            return Err(RecordingError::TooManyFrames {
                expected: self.output.frame_count,
            });
        }
        self.output.write_frame(&image, duration)?;
        self.recorded_frames += 1;
        Ok(())
    }

    /// Completes the APNG after every required frame has been recorded.
    pub fn finish(self) -> Result<(), RecordingError> {
        self.require_complete()?;
        self.output.finish()
    }

    fn require_complete(&self) -> Result<(), RecordingError> {
        if !self.is_complete() {
            return Err(RecordingError::Incomplete {
                expected: self.output.frame_count,
                actual: self.recorded_frames,
            });
        }
        Ok(())
    }
}

struct AnimatedPngWriter<W: Write> {
    writer: png::Writer<W>,
    size: SurfaceSize,
    frame_count: u32,
    written_frames: u32,
}

impl<W: Write> AnimatedPngWriter<W> {
    fn new(output: W, size: SurfaceSize, frame_count: u32) -> Result<Self, RecordingError> {
        let mut encoder = png::Encoder::new(output, size.width, size.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        encoder.set_animated(frame_count, 1)?;
        Ok(Self {
            writer: encoder.write_header()?,
            size,
            frame_count,
            written_frames: 0,
        })
    }

    fn write_frame(
        &mut self,
        image: &Srgba8Image,
        duration: Duration,
    ) -> Result<(), RecordingError> {
        self.validate_frame(image)?;
        let (numerator, denominator) = apng_delay(duration)?;
        self.writer.set_frame_delay(numerator, denominator)?;
        self.writer.write_image_data(image.pixels())?;
        self.written_frames += 1;
        Ok(())
    }

    fn validate_frame(&self, image: &Srgba8Image) -> Result<(), RecordingError> {
        if image.width() != self.size.width || image.height() != self.size.height {
            return Err(RecordingError::FrameSize {
                expected: self.size,
                actual: SurfaceSize {
                    width: image.width(),
                    height: image.height(),
                },
            });
        }
        Ok(())
    }

    fn finish(self) -> Result<(), RecordingError> {
        if self.written_frames != self.frame_count {
            return Err(RecordingError::Incomplete {
                expected: self.frame_count,
                actual: self.written_frames,
            });
        }
        self.writer.finish()?;
        Ok(())
    }
}

fn apng_delay(duration: Duration) -> Result<(u16, u16), RecordingError> {
    const NANOS_PER_SECOND: u128 = 1_000_000_000;
    const MAX: u128 = u16::MAX as u128;

    let nanos = duration.as_nanos();
    if nanos == 0 {
        return Err(RecordingError::ZeroFrameDuration);
    }
    if nanos > MAX * NANOS_PER_SECOND {
        return Err(RecordingError::FrameDurationTooLarge(duration));
    }

    let denominator = MAX.min((MAX * NANOS_PER_SECOND / nanos).max(1));
    let numerator = ((nanos * denominator + NANOS_PER_SECOND / 2) / NANOS_PER_SECOND).clamp(1, MAX);
    Ok((numerator as u16, denominator as u16))
}

#[derive(Debug)]
pub enum RecordingError {
    Encode(png::EncodingError),
    TooManyFrames {
        expected: u32,
    },
    Incomplete {
        expected: u32,
        actual: u32,
    },
    ZeroFrameDuration,
    FrameDurationTooLarge(Duration),
    FrameSize {
        expected: SurfaceSize,
        actual: SurfaceSize,
    },
}

impl fmt::Display for RecordingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode(error) => write!(formatter, "APNG encoding failed: {error}"),
            Self::TooManyFrames { expected } => {
                write!(
                    formatter,
                    "the recording already contains all {expected} frames"
                )
            }
            Self::Incomplete { expected, actual } => write!(
                formatter,
                "the recording requires {expected} frames, but contains {actual}"
            ),
            Self::ZeroFrameDuration => {
                formatter.write_str("an APNG frame duration must be positive")
            }
            Self::FrameDurationTooLarge(duration) => {
                write!(
                    formatter,
                    "APNG cannot represent frame duration {duration:?}"
                )
            }
            Self::FrameSize { expected, actual } => write!(
                formatter,
                "frame extent must be {}x{}, got {}x{}",
                expected.width, expected.height, actual.width, actual.height
            ),
        }
    }
}

impl Error for RecordingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Encode(error) => Some(error),
            _ => None,
        }
    }
}

impl From<png::EncodingError> for RecordingError {
    fn from(error: png::EncodingError) -> Self {
        Self::Encode(error)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn image(value: u8) -> Srgba8Image {
        Srgba8Image::new(1, 1, vec![value, 0, 0, 255]).unwrap()
    }

    #[test]
    fn frame_durations_are_approximated_with_apng_precision() {
        let requested = Duration::from_secs_f64(1.0 / 60.0);
        let (numerator, denominator) = apng_delay(requested).unwrap();
        let encoded = Duration::from_secs_f64(f64::from(numerator) / f64::from(denominator));

        assert!(encoded.abs_diff(requested) < Duration::from_micros(20));
        assert!(matches!(
            apng_delay(Duration::ZERO),
            Err(RecordingError::ZeroFrameDuration)
        ));
    }

    #[test]
    fn recorder_emits_timed_srgb_animated_png_frames() {
        let mut bytes = Vec::new();
        {
            let mut recorder = AnimatedPngRecorder::new(
                &mut bytes,
                SurfaceSize {
                    width: 1,
                    height: 1,
                },
                2,
            )
            .unwrap();
            recorder
                .record(image(10), Duration::from_millis(16))
                .unwrap();
            recorder
                .record(image(20), Duration::from_millis(17))
                .unwrap();
            recorder.finish().unwrap();
        }

        let decoder = png::Decoder::new(Cursor::new(bytes));
        let mut reader = decoder.read_info().unwrap();
        assert_eq!(
            reader.info().srgb,
            Some(png::SrgbRenderingIntent::Perceptual)
        );
        assert_eq!(reader.info().animation_control.unwrap().num_frames, 2);

        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut pixels).unwrap();
        let first = reader.info().frame_control.unwrap();
        assert_eq!(&pixels[..4], &[10, 0, 0, 255]);
        assert_eq!(
            (first.delay_num, first.delay_den),
            apng_delay(Duration::from_millis(16)).unwrap()
        );

        reader.next_frame(&mut pixels).unwrap();
        let second = reader.info().frame_control.unwrap();
        assert_eq!(&pixels[..4], &[20, 0, 0, 255]);
        assert_eq!(
            (second.delay_num, second.delay_den),
            apng_delay(Duration::from_millis(17)).unwrap()
        );
    }

    #[test]
    fn recorder_rejects_invalid_frames_without_advancing() {
        let mut bytes = Vec::new();
        let mut recorder = AnimatedPngRecorder::new(
            &mut bytes,
            SurfaceSize {
                width: 1,
                height: 1,
            },
            2,
        )
        .unwrap();

        let wrong_size = Srgba8Image::new(2, 1, vec![0; 8]).unwrap();
        assert!(matches!(
            recorder.record(wrong_size, Duration::ZERO),
            Err(RecordingError::FrameSize { .. })
        ));
        assert_eq!(recorder.recorded_frames(), 0);

        assert!(matches!(
            recorder.record(image(10), Duration::ZERO),
            Err(RecordingError::ZeroFrameDuration)
        ));
        assert_eq!(recorder.recorded_frames(), 0);

        assert!(matches!(
            recorder.finish(),
            Err(RecordingError::Incomplete {
                expected: 2,
                actual: 0
            })
        ));
    }

    #[test]
    fn recorder_rejects_frames_beyond_its_declared_count() {
        let mut bytes = Vec::new();
        let mut recorder = AnimatedPngRecorder::new(
            &mut bytes,
            SurfaceSize {
                width: 1,
                height: 1,
            },
            2,
        )
        .unwrap();

        recorder
            .record(image(10), Duration::from_millis(1))
            .unwrap();
        recorder
            .record(image(20), Duration::from_millis(1))
            .unwrap();
        assert!(matches!(
            recorder.record(image(30), Duration::from_millis(1)),
            Err(RecordingError::TooManyFrames { expected: 2 })
        ));
        recorder.finish().unwrap();
    }
}
