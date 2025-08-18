#![cfg_attr(test, feature(test))]
// #![cfg_attr(any(target_arch = "arm", target_arch = "aarch64"), feature(stdsimd))] // Disabled for stable builds

#[cfg(test)]
extern crate test;

mod constants;
mod haraka256;
mod haraka512;

// Conditional compilation for SIMD vs portable
#[cfg(any(target_arch = "wasm32", feature = "portable"))]
mod portable_aes;
#[cfg(any(target_arch = "wasm32", feature = "portable"))]
mod simd128_portable;
#[cfg(any(target_arch = "wasm32", feature = "portable"))]
use simd128_portable as simd128;

#[cfg(not(any(target_arch = "wasm32", feature = "portable")))]
mod simd128;

pub fn haraka256<const N_ROUNDS: usize>(dst: &mut [u8; 32], src: &[u8; 32]) {
    haraka256::haraka256::<{ N_ROUNDS }>(dst, src)
}

pub fn haraka512<const N_ROUNDS: usize>(dst: &mut [u8; 32], src: &[u8; 64]) {
    haraka512::haraka512::<{ N_ROUNDS }>(dst, src)
}
