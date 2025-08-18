use crate::portable_aes::aesenc_soft;

/// Portable software implementation of 128-bit SIMD operations
#[derive(Clone, Copy)]
pub(crate) struct Simd128([u8; 16]);

impl Simd128 {
    pub const fn from(x: u128) -> Self {
        Self(x.to_le_bytes())
    }

    /// Read from array pointer
    #[inline(always)]
    pub fn read(src: &[u8; 16]) -> Self {
        let mut data = [0u8; 16];
        data.copy_from_slice(src);
        Self(data)
    }

    /// Write into array pointer
    #[inline(always)]
    pub fn write(self, dst: &mut [u8; 16]) {
        dst.copy_from_slice(&self.0);
    }

    /// AES encryption round function
    #[inline(always)]
    pub(crate) fn aesenc(block: &mut Self, key: &Self) {
        aesenc_soft(&mut block.0, &key.0);
    }

    /// XOR operation
    #[inline(always)]
    pub(crate) fn pxor(dst: &mut Self, src: &Self) {
        for (d, s) in dst.0.iter_mut().zip(src.0.iter()) {
            *d ^= s;
        }
    }

    /// Unpack low 32-bit integers
    #[inline(always)]
    pub(crate) fn unpacklo_epi32(dst: &mut Self, src: &Self) {
        let mut result = [0u8; 16];

        // Take alternating 32-bit values starting from low
        result[0..4].copy_from_slice(&dst.0[0..4]); // dst[0]
        result[4..8].copy_from_slice(&src.0[0..4]); // src[0]
        result[8..12].copy_from_slice(&dst.0[4..8]); // dst[1]
        result[12..16].copy_from_slice(&src.0[4..8]); // src[1]

        dst.0 = result;
    }

    /// Unpack high 32-bit integers
    #[inline(always)]
    pub(crate) fn unpackhi_epi32(dst: &mut Self, src: &Self) {
        let mut result = [0u8; 16];

        // Take alternating 32-bit values starting from high
        result[0..4].copy_from_slice(&dst.0[8..12]); // dst[2]
        result[4..8].copy_from_slice(&src.0[8..12]); // src[2]
        result[8..12].copy_from_slice(&dst.0[12..16]); // dst[3]
        result[12..16].copy_from_slice(&src.0[12..16]); // src[3]

        dst.0 = result;
    }

    /// Unpack low 64-bit integers
    #[inline(always)]
    pub(crate) fn unpacklo_epi64(lhs: &Self, rhs: &Self) -> Self {
        let mut result = [0u8; 16];

        // Take low 64 bits from each
        result[0..8].copy_from_slice(&lhs.0[0..8]); // lhs low
        result[8..16].copy_from_slice(&rhs.0[0..8]); // rhs low

        Self(result)
    }

    /// Unpack high 64-bit integers
    #[inline(always)]
    pub(crate) fn unpackhi_epi64(lhs: &Self, rhs: &Self) -> Self {
        let mut result = [0u8; 16];

        // Take high 64 bits from each
        result[0..8].copy_from_slice(&lhs.0[8..16]); // lhs high
        result[8..16].copy_from_slice(&rhs.0[8..16]); // rhs high

        Self(result)
    }
}
