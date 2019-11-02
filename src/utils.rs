use crate::communication::TxBuffer;

pub fn fill_with_bytes(buffer: &mut TxBuffer, arg: &[u8]) {
    buffer.extend_from_slice(arg).unwrap();
}

pub fn fill_with_str(buffer: &mut TxBuffer, arg: &str) {
    buffer.extend_from_slice(arg.as_bytes()).unwrap();
}

pub fn to_rads(d: f32) -> f32 {
    d * 3.141592 / 180.
}
