use crate::errors::Error;

/// Encodes `message` as a frame of the Cast protocol: its length (as a big endian `u32`), followed
/// by the message.
pub fn to_frame<M: protobuf::Message>(message: &M) -> Result<Vec<u8>, Error> {
    let length = message.compute_size() as u32;
    let mut frame = Vec::with_capacity(4 + length as usize);

    frame.extend_from_slice(&length.to_be_bytes());
    message.write_to_vec(&mut frame)?;

    Ok(frame)
}
