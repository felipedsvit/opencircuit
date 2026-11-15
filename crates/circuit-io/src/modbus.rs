//! Modbus TCP frames: the MBAP header and the protocol data unit of the
//! functions the server supports (ADR-0009).
//!
//! Decoding never panics and never trusts a length it has not checked; every
//! malformed frame is an error the server answers by closing the connection,
//! and every unsupported function or bad address is a Modbus exception.

/// Largest application data unit Modbus TCP allows: 7 bytes of header and
/// 253 of protocol data.
pub const MAX_FRAME: usize = 260;
/// Bytes of the MBAP header, unit identifier included.
const HEADER: usize = 7;
/// Most coils or discrete inputs one read may ask for.
pub const MAX_READ_BITS: u16 = 2000;
/// Most registers one read may ask for.
pub const MAX_READ_REGISTERS: u16 = 125;
/// Most coils one write may set.
pub const MAX_WRITE_COILS: u16 = 1968;

/// Function codes the server answers.
pub mod function {
    /// Read coils.
    pub const READ_COILS: u8 = 1;
    /// Read discrete inputs.
    pub const READ_DISCRETE_INPUTS: u8 = 2;
    /// Read input registers.
    pub const READ_INPUT_REGISTERS: u8 = 4;
    /// Write single coil.
    pub const WRITE_SINGLE_COIL: u8 = 5;
    /// Write multiple coils.
    pub const WRITE_MULTIPLE_COILS: u8 = 15;
}

/// Modbus exception codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Exception {
    /// The function is not supported.
    IllegalFunction = 1,
    /// An address is outside the map.
    IllegalDataAddress = 2,
    /// A count or a value is not allowed.
    IllegalDataValue = 3,
}

/// The MBAP header of a frame, echoed in the answer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Header {
    /// Transaction identifier chosen by the client.
    pub transaction: u16,
    /// Unit identifier.
    pub unit: u8,
}

/// A decoded request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Request {
    /// Function 1.
    ReadCoils {
        /// First address.
        start: u16,
        /// Number of coils.
        count: u16,
    },
    /// Function 2.
    ReadDiscreteInputs {
        /// First address.
        start: u16,
        /// Number of inputs.
        count: u16,
    },
    /// Function 4.
    ReadInputRegisters {
        /// First address.
        start: u16,
        /// Number of registers.
        count: u16,
    },
    /// Function 5.
    WriteSingleCoil {
        /// Address.
        address: u16,
        /// New state.
        value: bool,
    },
    /// Function 15.
    WriteMultipleCoils {
        /// First address.
        start: u16,
        /// New states, in address order.
        values: Vec<bool>,
    },
    /// A well-formed request the server cannot honour: answered with an
    /// exception.
    Refused {
        /// The function code of the request.
        function: u8,
        /// Why.
        exception: Exception,
    },
}

/// An answer to encode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Response {
    /// Bits read (functions 1 and 2).
    Bits {
        /// The function code answered.
        function: u8,
        /// The bits, in address order.
        bits: Vec<bool>,
    },
    /// Registers read (function 4).
    Registers(Vec<u16>),
    /// Echo of a single coil write (function 5).
    CoilWritten {
        /// Address.
        address: u16,
        /// State written.
        value: bool,
    },
    /// Echo of a multiple coil write (function 15).
    CoilsWritten {
        /// First address.
        start: u16,
        /// Number of coils written.
        count: u16,
    },
    /// An exception.
    Exception {
        /// The function code of the request.
        function: u8,
        /// The exception.
        exception: Exception,
    },
}

/// A frame that cannot be a Modbus TCP request: the server closes the
/// connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameError {
    /// The protocol identifier is not 0.
    NotModbus,
    /// The length field is outside what Modbus TCP allows.
    BadLength,
    /// The frame ends before its declared length, or the data of a function
    /// is cut short or too long.
    Truncated,
}

fn word(bytes: &[u8], at: usize) -> Option<u16> {
    let high = *bytes.get(at)?;
    let low = *bytes.get(at.checked_add(1)?)?;
    Some(u16::from_be_bytes([high, low]))
}

/// How many bytes the frame at the start of `buffer` has, once its header
/// has arrived: `None` while fewer than 7 bytes are buffered.
///
/// # Errors
///
/// Returns [`FrameError`] when the header cannot start a Modbus TCP frame.
pub fn frame_len(buffer: &[u8]) -> Option<Result<usize, FrameError>> {
    if buffer.len() < HEADER {
        return None;
    }
    let (Some(protocol), Some(length)) = (word(buffer, 2), word(buffer, 4)) else {
        return None;
    };
    if protocol != 0 {
        return Some(Err(FrameError::NotModbus));
    }
    // The length counts the unit identifier and the PDU: 2 to 254 bytes.
    let length = usize::from(length);
    if !(2..=MAX_FRAME.saturating_sub(6)).contains(&length) {
        return Some(Err(FrameError::BadLength));
    }
    Some(Ok(length.saturating_add(6)))
}

/// Decode one complete frame.
///
/// # Errors
///
/// Returns [`FrameError`] when the bytes are not one complete Modbus TCP
/// frame. A complete frame with an unsupported function or an impossible
/// count decodes to [`Request::Refused`].
pub fn decode(frame: &[u8]) -> Result<(Header, Request), FrameError> {
    let length = match frame_len(frame) {
        None => return Err(FrameError::Truncated),
        Some(length) => length?,
    };
    if frame.len() != length {
        return Err(FrameError::Truncated);
    }
    let header = Header {
        transaction: word(frame, 0).ok_or(FrameError::Truncated)?,
        unit: *frame.get(6).ok_or(FrameError::Truncated)?,
    };
    let pdu = frame.get(HEADER..).ok_or(FrameError::Truncated)?;
    let function = *pdu.first().ok_or(FrameError::Truncated)?;
    let data = pdu.get(1..).ok_or(FrameError::Truncated)?;
    let refused = |exception| Request::Refused {
        function,
        exception,
    };
    let pair = || -> Result<(u16, u16), FrameError> {
        if data.len() != 4 {
            return Err(FrameError::Truncated);
        }
        Ok((
            word(data, 0).ok_or(FrameError::Truncated)?,
            word(data, 2).ok_or(FrameError::Truncated)?,
        ))
    };
    let request = match function {
        function::READ_COILS | function::READ_DISCRETE_INPUTS => {
            let (start, count) = pair()?;
            if count == 0 || count > MAX_READ_BITS {
                refused(Exception::IllegalDataValue)
            } else if function == function::READ_COILS {
                Request::ReadCoils { start, count }
            } else {
                Request::ReadDiscreteInputs { start, count }
            }
        }
        function::READ_INPUT_REGISTERS => {
            let (start, count) = pair()?;
            if count == 0 || count > MAX_READ_REGISTERS {
                refused(Exception::IllegalDataValue)
            } else {
                Request::ReadInputRegisters { start, count }
            }
        }
        function::WRITE_SINGLE_COIL => {
            let (address, value) = pair()?;
            match value {
                0xFF00 => Request::WriteSingleCoil {
                    address,
                    value: true,
                },
                0x0000 => Request::WriteSingleCoil {
                    address,
                    value: false,
                },
                _ => refused(Exception::IllegalDataValue),
            }
        }
        function::WRITE_MULTIPLE_COILS => {
            let start = word(data, 0).ok_or(FrameError::Truncated)?;
            let count = word(data, 2).ok_or(FrameError::Truncated)?;
            let byte_count = usize::from(*data.get(4).ok_or(FrameError::Truncated)?);
            let bytes = data.get(5..).ok_or(FrameError::Truncated)?;
            if bytes.len() != byte_count {
                return Err(FrameError::Truncated);
            }
            if count == 0 || count > MAX_WRITE_COILS || byte_count != usize::from(count).div_ceil(8)
            {
                refused(Exception::IllegalDataValue)
            } else {
                let values = (0..usize::from(count))
                    .map(|bit| {
                        bytes
                            .get(bit / 8)
                            .is_some_and(|byte| byte & (1 << (bit % 8)) != 0)
                    })
                    .collect();
                Request::WriteMultipleCoils { start, values }
            }
        }
        _ => refused(Exception::IllegalFunction),
    };
    Ok((header, request))
}

/// Encode an answer with the header of its request.
#[must_use]
pub fn encode(header: Header, response: &Response) -> Vec<u8> {
    let mut pdu: Vec<u8> = Vec::new();
    match response {
        Response::Bits { function, bits } => {
            let mut packed = vec![0_u8; bits.len().div_ceil(8)];
            for (index, bit) in bits.iter().enumerate() {
                if let (true, Some(byte)) = (*bit, packed.get_mut(index / 8)) {
                    *byte |= 1 << (index % 8);
                }
            }
            pdu.push(*function);
            pdu.push(u8::try_from(packed.len()).unwrap_or(u8::MAX));
            pdu.extend(packed);
        }
        Response::Registers(registers) => {
            pdu.push(function::READ_INPUT_REGISTERS);
            pdu.push(u8::try_from(registers.len().saturating_mul(2)).unwrap_or(u8::MAX));
            for register in registers {
                pdu.extend(register.to_be_bytes());
            }
        }
        Response::CoilWritten { address, value } => {
            pdu.push(function::WRITE_SINGLE_COIL);
            pdu.extend(address.to_be_bytes());
            pdu.extend(if *value { [0xFF, 0x00] } else { [0x00, 0x00] });
        }
        Response::CoilsWritten { start, count } => {
            pdu.push(function::WRITE_MULTIPLE_COILS);
            pdu.extend(start.to_be_bytes());
            pdu.extend(count.to_be_bytes());
        }
        Response::Exception {
            function,
            exception,
        } => {
            pdu.push(function | 0x80);
            pdu.push(*exception as u8);
        }
    }
    let length = u16::try_from(pdu.len().saturating_add(1)).unwrap_or(u16::MAX);
    let mut frame = Vec::with_capacity(pdu.len().saturating_add(HEADER));
    frame.extend(header.transaction.to_be_bytes());
    frame.extend([0, 0]);
    frame.extend(length.to_be_bytes());
    frame.push(header.unit);
    frame.extend(pdu);
    frame
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn frame(transaction: u16, unit: u8, pdu: &[u8]) -> Vec<u8> {
        let mut bytes = transaction.to_be_bytes().to_vec();
        bytes.extend([0, 0]);
        bytes.extend(u16::try_from(pdu.len() + 1).unwrap().to_be_bytes());
        bytes.push(unit);
        bytes.extend(pdu);
        bytes
    }

    #[test]
    fn read_requests_decode_with_their_header() {
        let (header, request) = decode(&frame(7, 1, &[1, 0, 2, 0, 3])).unwrap();

        assert_eq!(
            header,
            Header {
                transaction: 7,
                unit: 1
            }
        );
        assert_eq!(request, Request::ReadCoils { start: 2, count: 3 });
        assert_eq!(
            decode(&frame(1, 1, &[4, 0, 0, 0, 5])).unwrap().1,
            Request::ReadInputRegisters { start: 0, count: 5 }
        );
    }

    #[test]
    fn coil_writes_decode_their_values() {
        assert_eq!(
            decode(&frame(1, 1, &[5, 0, 4, 0xFF, 0x00])).unwrap().1,
            Request::WriteSingleCoil {
                address: 4,
                value: true
            }
        );
        assert_eq!(
            decode(&frame(1, 1, &[15, 0, 1, 0, 10, 2, 0b0000_0101, 0b10]))
                .unwrap()
                .1,
            Request::WriteMultipleCoils {
                start: 1,
                values: vec![
                    true, false, true, false, false, false, false, false, false, true
                ],
            }
        );
    }

    #[test]
    fn unsupported_functions_and_bad_counts_are_refused_not_dropped() {
        for (pdu, exception) in [
            (vec![3, 0, 0, 0, 1], Exception::IllegalFunction),
            (vec![6, 0, 0, 0, 1], Exception::IllegalFunction),
            (vec![1, 0, 0, 0, 0], Exception::IllegalDataValue),
            (vec![1, 0, 0, 0x07, 0xD1], Exception::IllegalDataValue),
            (vec![5, 0, 0, 0x12, 0x34], Exception::IllegalDataValue),
            (vec![15, 0, 0, 0, 9, 1, 0xFF], Exception::IllegalDataValue),
        ] {
            let (_, request) = decode(&frame(1, 1, &pdu)).unwrap();

            assert_eq!(
                request,
                Request::Refused {
                    function: pdu[0],
                    exception
                },
                "{pdu:?}"
            );
        }
    }

    #[test]
    fn frames_that_are_not_modbus_tcp_are_errors() {
        let mut other_protocol = frame(1, 1, &[1, 0, 0, 0, 1]);
        other_protocol[3] = 1;
        let mut short = frame(1, 1, &[1, 0, 0, 0, 1]);
        short.pop();
        let mut absurd = frame(1, 1, &[1, 0, 0, 0, 1]);
        absurd[4] = 0x10;

        assert_eq!(decode(&other_protocol), Err(FrameError::NotModbus));
        assert_eq!(decode(&short), Err(FrameError::Truncated));
        assert_eq!(decode(&absurd), Err(FrameError::BadLength));
        assert_eq!(frame_len(&[0, 1, 0]), None);
    }

    #[test]
    fn answers_encode_as_the_standard_frames() {
        let header = Header {
            transaction: 0x1234,
            unit: 1,
        };

        assert_eq!(
            encode(
                header,
                &Response::Bits {
                    function: 1,
                    bits: vec![true, false, true]
                }
            ),
            [0x12, 0x34, 0, 0, 0, 4, 1, 1, 1, 0b101]
        );
        assert_eq!(
            encode(header, &Response::Registers(vec![0x0102])),
            [0x12, 0x34, 0, 0, 0, 5, 1, 4, 2, 1, 2]
        );
        assert_eq!(
            encode(
                header,
                &Response::Exception {
                    function: 3,
                    exception: Exception::IllegalFunction
                }
            ),
            [0x12, 0x34, 0, 0, 0, 3, 1, 0x83, 1]
        );
    }

    proptest! {
        #[test]
        fn arbitrary_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..300)) {
            let _ = frame_len(&bytes);
            let _ = decode(&bytes);
        }

        #[test]
        fn every_bit_read_answer_round_trips(bits in prop::collection::vec(any::<bool>(), 1..2000)) {
            let header = Header { transaction: 1, unit: 1 };
            let encoded = encode(header, &Response::Bits { function: 2, bits: bits.clone() });
            let packed = &encoded[9..];

            prop_assert_eq!(usize::from(encoded[8]), bits.len().div_ceil(8));
            for (index, bit) in bits.iter().enumerate() {
                prop_assert_eq!(packed[index / 8] & (1 << (index % 8)) != 0, *bit);
            }
        }
    }
}
