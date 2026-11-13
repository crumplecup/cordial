//! What a client and a session server say to each other over the socket.
//!
//! A request is one line of JSON. The answer is a stream of frames: output as
//! the command writes it, then the exit code, or a refusal that tells the client
//! to run the command itself.

use super::Stream;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use tracing::instrument;

/// Bump when a request or frame changes shape; a server refuses any other.
pub const PROTOCOL: u32 = 1;

/// The reason a server gives when it was started from another build than the
/// client's, so the client replaces it rather than running the command itself.
pub const STALE_BUILD: &str = "the session runs another build of cordial";

/// One command a client asks the session to run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct Request {
    /// The protocol version the client speaks.
    #[getter(copy)]
    version: u32,
    /// The command line, program name first, with the project made explicit.
    argv: Vec<String>,
    /// The environment variables that shape a build, as the client has them.
    env: BTreeMap<String, String>,
    /// The cordial executable the client is, as the run cache fingerprints it. A
    /// server started from another build refuses, so a rebuilt cordial never
    /// answers from the old code.
    build: String,
    /// Ask the server to exit instead of running a command.
    #[getter(copy)]
    shutdown: bool,
}

impl Request {
    /// A request to run `argv` with the client's build-shaping `env`.
    #[instrument(level = "trace", skip(argv, env))]
    pub fn command(argv: Vec<String>, env: BTreeMap<String, String>) -> Self {
        Self {
            version: PROTOCOL,
            argv,
            env,
            build: crate::run_cache::executable_fingerprint(),
            shutdown: false,
        }
    }

    /// A request that the server exit.
    #[instrument(level = "trace")]
    pub fn stop() -> Self {
        Self {
            version: PROTOCOL,
            argv: Vec::new(),
            env: BTreeMap::new(),
            build: String::new(),
            shutdown: true,
        }
    }
}

/// One piece of a server's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// Bytes the command wrote to a stream.
    Output(Stream, Vec<u8>),
    /// The command finished with this exit code.
    Exit(i32),
    /// The server will not run this command; the client should.
    Refused(String),
}

const STDOUT: u8 = 1;
const STDERR: u8 = 2;
const EXIT: u8 = 3;
const REFUSED: u8 = 4;

/// Write one frame: a kind byte, a little-endian length, then the payload.
#[instrument(level = "trace", skip(out, frame), err(level = "warn"))]
pub fn write_frame(out: &mut impl Write, frame: &Frame) -> io::Result<()> {
    let (kind, payload): (u8, Vec<u8>) = match frame {
        Frame::Output(Stream::Stdout, bytes) => (STDOUT, bytes.clone()),
        Frame::Output(Stream::Stderr, bytes) => (STDERR, bytes.clone()),
        Frame::Exit(code) => (EXIT, code.to_le_bytes().to_vec()),
        Frame::Refused(reason) => (REFUSED, reason.as_bytes().to_vec()),
    };
    let length = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "frame too large"))?;
    out.write_all(&[kind])?;
    out.write_all(&length.to_le_bytes())?;
    out.write_all(&payload)?;
    out.flush()
}

/// Read the next frame, or `None` at a clean end of stream.
#[instrument(level = "trace", skip(input), err(level = "warn"))]
pub fn read_frame(input: &mut impl Read) -> io::Result<Option<Frame>> {
    let mut kind = [0_u8; 1];
    if input.read(&mut kind)? == 0 {
        return Ok(None);
    }
    let mut length = [0_u8; 4];
    input.read_exact(&mut length)?;
    let mut payload = vec![0_u8; u32::from_le_bytes(length) as usize];
    input.read_exact(&mut payload)?;
    let invalid = |what: &str| io::Error::new(io::ErrorKind::InvalidData, what.to_owned());
    Ok(Some(match kind[0] {
        STDOUT => Frame::Output(Stream::Stdout, payload),
        STDERR => Frame::Output(Stream::Stderr, payload),
        EXIT => Frame::Exit(i32::from_le_bytes(
            payload.try_into().map_err(|_| invalid("exit code"))?,
        )),
        REFUSED => Frame::Refused(String::from_utf8_lossy(&payload).into_owned()),
        _ => return Err(invalid("unknown frame")),
    }))
}
