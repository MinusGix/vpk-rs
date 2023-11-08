use std::io::Read;

pub(crate) fn read_u16(r: &mut impl Read) -> std::io::Result<u16> {
    let mut val = [0; 2];

    r.read_exact(&mut val)?;

    Ok(u16::from_le_bytes(val))
}

pub(crate) fn read_u32(r: &mut impl Read) -> std::io::Result<u32> {
    let mut val = [0; 4];

    r.read_exact(&mut val)?;

    Ok(u32::from_le_bytes(val))
}

pub(crate) fn read_u128(r: &mut impl Read) -> std::io::Result<u128> {
    let mut val = [0; 16];

    r.read_exact(&mut val)?;

    Ok(u128::from_le_bytes(val))
}
