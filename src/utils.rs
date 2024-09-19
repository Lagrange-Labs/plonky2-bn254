use bitvec::{order::Lsb0, view::BitView};
use itertools::Itertools;
use num::BigUint;
use plonky2::{
    hash::hash_types::RichField,
    iop::{target::Target, witness::Witness},
};

pub fn biguint_to_bits(x: &BigUint, len: usize) -> Vec<bool> {
    let limbs = x.to_bytes_le();
    let mut bits = vec![];
    for limb in limbs {
        let limb_bits = limb.view_bits::<Lsb0>().iter().map(|b| *b).collect_vec();
        bits.extend(limb_bits);
    }
    assert!(bits.len() <= len);
    let to_padd = vec![false; len - bits.len()];
    bits.extend(to_padd);
    bits
}

pub fn bits_to_biguint(bits: &[bool]) -> BigUint {
    let mut limbs = vec![];
    for chunk in bits.chunks(8) {
        let mut limb = 0u8;
        for (i, bit) in chunk.iter().enumerate() {
            if *bit {
                limb |= 1 << i;
            }
        }
        limbs.push(limb);
    }
    BigUint::from_bytes_le(&limbs)
}

pub fn u32_digits_to_biguint(inputs: &[u32]) -> BigUint {
    let mut bits = vec![];
    for limb in inputs {
        let limb_bits = limb.view_bits::<Lsb0>().iter().map(|b| *b).collect_vec();
        bits.extend(limb_bits);
    }
    bits_to_biguint(&bits)
}

pub fn get_u256_biguint<F: RichField, W: Witness<F>>(pw: &W, x: &[Target]) -> BigUint {
    assert!(x.len() <= 8);
    let x_value = x
        .iter()
        .map(|x| pw.get_target(*x).to_canonical_u64() as u32)
        .collect_vec();
    u32_digits_to_biguint(&x_value)
}
