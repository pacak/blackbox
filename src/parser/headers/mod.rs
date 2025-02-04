use super::frame::{
    is_frame_def_header, parse_frame_def_header, FrameKind, MainFrameDef, MainFrameDefBuilder,
    SlowFrameDef, SlowFrameDefBuilder,
};
use super::reader::ByteReader;
use super::{ParseError, ParseResult, Reader};
use crate::LogVersion;
use std::iter;
use std::str::{self, FromStr};

#[allow(dead_code)]
#[derive(Debug)]
pub struct Headers<'data> {
    pub version: LogVersion,
    pub(crate) main_frames: MainFrameDef<'data>,
    pub(crate) slow_frames: SlowFrameDef<'data>,

    pub firmware_revision: &'data str,
    pub firmware_kind: FirmwareKind,
    pub board_info: &'data str,
    pub craft_name: &'data str,

    /// Measured battery voltage at arm
    pub vbat_reference: u16,
    pub min_throttle: u16,
    pub motor_output_range: MotorOutputRange,
}

impl<'data> Headers<'data> {
    pub fn main_fields(&self) -> impl Iterator<Item = &str> {
        iter::once(self.main_frames.iteration.name)
            .chain(iter::once(self.main_frames.time.name))
            .chain(self.main_frames.fields.iter().map(|f| f.name))
    }

    pub fn slow_fields(&self) -> impl Iterator<Item = &str> {
        self.slow_frames.0.iter().map(|f| f.name)
    }

    pub(crate) fn parse(data: &mut Reader<'data>) -> ParseResult<Self> {
        let bytes = &mut data.bytes();

        let (name, _product) = parse_header(bytes)?;
        assert_eq!(name, "Product", "`Product` header must be first");

        let (name, version) = parse_header(bytes)?;
        assert_eq!(name, "Data version", "`Data version` header must be second");
        let version = version.parse().map_err(|_| ParseError::InvalidHeader {
            header: name.to_owned(),
            value: version.to_owned(),
        })?;

        let mut state = State::new(version);

        loop {
            match bytes.peek() {
                Some(b'H') => {}
                Some(_) => break,
                None => return Err(ParseError::UnexpectedEof),
            }

            let (name, value) = parse_header(bytes)?;
            state.update(name, value)?;
        }

        state.finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FirmwareKind {
    Baseflight,
    Cleanflight,
    INav,
}

impl FromStr for FirmwareKind {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "cleanflight" => Ok(Self::Cleanflight),
            "baseflight" => Ok(Self::Baseflight),
            "inav" => Ok(Self::INav),
            _ => Err(ParseError::InvalidHeader {
                header: "Firmware type".to_owned(),
                value: s.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MotorOutputRange(u16, u16);

impl MotorOutputRange {
    pub const fn new(min: u16, max: u16) -> Self {
        Self(min, max)
    }

    pub const fn min(&self) -> u16 {
        self.0
    }

    pub const fn max(&self) -> u16 {
        self.1
    }
}

impl FromStr for MotorOutputRange {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.split_once(',')
            .and_then(|(min, max)| {
                let min = min.parse().ok()?;
                let max = max.parse().ok()?;
                Some(MotorOutputRange::new(min, max))
            })
            .ok_or(ParseError::Corrupted)
    }
}

#[derive(Debug)]
struct State<'data> {
    version: LogVersion,
    main_frames: MainFrameDefBuilder<'data>,
    slow_frames: SlowFrameDefBuilder<'data>,

    firmware_revision: Option<&'data str>,
    firmware_kind: Option<FirmwareKind>,
    board_info: Option<&'data str>,
    craft_name: Option<&'data str>,

    vbat_reference: Option<u16>,
    min_throttle: Option<u16>,
    motor_output_range: Option<MotorOutputRange>,
}

impl<'data> State<'data> {
    fn new(version: LogVersion) -> Self {
        Self {
            version,
            main_frames: MainFrameDef::builder(),
            slow_frames: SlowFrameDef::builder(),

            firmware_revision: None,
            firmware_kind: None,
            board_info: None,
            craft_name: None,

            vbat_reference: None,
            min_throttle: None,
            motor_output_range: None,
        }
    }

    fn update(&mut self, header: &'data str, value: &'data str) -> ParseResult<()> {
        match header.to_ascii_lowercase().as_str() {
            "firmware revision" => self.firmware_revision = Some(value),
            "firmware type" => self.firmware_kind = Some(value.parse()?),
            "board information" => self.board_info = Some(value),
            "craft name" => self.craft_name = Some(value),

            "vbatref" => {
                let vbat_reference = value
                    .parse()
                    .map_err(|_| ParseError::invalid_header(header, value))?;
                self.vbat_reference = Some(vbat_reference);
            }
            "minthrottle" => {
                let min_throttle = value
                    .parse()
                    .map_err(|_| ParseError::invalid_header(header, value))?;
                self.min_throttle = Some(min_throttle);
            }
            "motoroutput" => {
                let range = value
                    .parse()
                    .map_err(|_| ParseError::invalid_header(header, value))?;
                self.motor_output_range = Some(range);
            }

            _ if is_frame_def_header(header) => {
                let (frame_kind, property) = parse_frame_def_header(header).unwrap();

                match frame_kind {
                    FrameKind::Inter | FrameKind::Intra => {
                        self.main_frames.update(frame_kind, property, value);
                    }
                    FrameKind::Slow => self.slow_frames.update(property, value),
                }
            }
            header => tracing::debug!("skipping unknown header: `{header}` = `{value}`"),
        }

        Ok(())
    }

    fn finish(self) -> ParseResult<Headers<'data>> {
        Ok(Headers {
            version: self.version,
            main_frames: self.main_frames.parse()?,
            slow_frames: self.slow_frames.parse()?,

            // TODO: return an error instead of unwrap
            firmware_revision: self.firmware_revision.unwrap(),
            firmware_kind: self.firmware_kind.unwrap(),
            board_info: self.board_info.unwrap(),
            craft_name: self.craft_name.unwrap(),

            vbat_reference: self.vbat_reference.unwrap(),
            min_throttle: self.min_throttle.unwrap(),
            motor_output_range: self.motor_output_range.unwrap(),
        })
    }
}

/// Expects the next character to be the leading H
fn parse_header<'data>(bytes: &mut ByteReader<'data, '_>) -> ParseResult<(&'data str, &'data str)> {
    match bytes.read_u8() {
        Some(b'H') => {}
        Some(_) => return Err(ParseError::Corrupted),
        None => return Err(ParseError::UnexpectedEof),
    }

    let line = bytes.read_line().ok_or(ParseError::UnexpectedEof)?;

    let line = str::from_utf8(line)?;
    let line = line.strip_prefix(' ').unwrap_or(line);
    let (name, value) = line.split_once(':').ok_or(ParseError::HeaderMissingColon)?;

    tracing::trace!("read header `{name}` = `{value}`");

    Ok((name, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "HeaderInvalidUtf8")]
    fn invalid_utf8() {
        let mut b = Reader::new(b"H \xFF:\xFF\n");
        parse_header(&mut b.bytes()).unwrap();
    }
}
