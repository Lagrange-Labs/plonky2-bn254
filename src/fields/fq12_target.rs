use ark_bn254::{Fq, Fq12};
use ark_ff::Field;
use itertools::Itertools;
use num_bigint::BigUint;
use plonky2::{
    field::extension::Extendable,
    hash::hash_types::RichField,
    iop::{
        generator::{GeneratedValues, SimpleGenerator},
        target::{BoolTarget, Target},
        witness::{PartitionWitness, Witness, WitnessWrite},
    },
    plonk::{circuit_builder::CircuitBuilder, circuit_data::CommonCircuitData},
    util::serialization::{Buffer, IoError, Read, Write},
};
use plonky2_ecdsa::gadgets::{
    biguint::{GeneratedValuesBigUint, WitnessBigUint},
    nonnative::CircuitBuilderNonNative,
};

use crate::{
    fields::{
        fq_target::FqTarget,
        native::{from_biguint_to_fq, MyFq12},
    },
    utils::get_u256_biguint,
};

#[derive(Clone, Debug, Default)]
pub struct Fq12Target<F: RichField + Extendable<D>, const D: usize> {
    pub coeffs: [FqTarget<F, D>; 12],
}

impl<F: RichField + Extendable<D>, const D: usize> Fq12Target<F, D> {
    pub fn empty(builder: &mut CircuitBuilder<F, D>) -> Self {
        let coeffs = [(); 12]
            .iter()
            .map(|_| FqTarget::empty(builder))
            .collect_vec()
            .try_into()
            .unwrap();
        Fq12Target { coeffs }
    }

    pub fn new(coeffs: Vec<FqTarget<F, D>>) -> Self {
        Fq12Target {
            coeffs: coeffs.try_into().unwrap(),
        }
    }

    pub fn connect(builder: &mut CircuitBuilder<F, D>, lhs: &Self, rhs: &Self) {
        for i in 0..12 {
            builder.connect_nonnative(&lhs.coeffs[i].target, &rhs.coeffs[i].target);
        }
    }

    pub fn select(
        builder: &mut CircuitBuilder<F, D>,
        a: &Self,
        b: &Self,
        flag: &BoolTarget,
    ) -> Self {
        let selected = a
            .coeffs
            .iter()
            .zip(b.coeffs.iter())
            .map(|(a, b)| FqTarget::select(builder, a, b, flag))
            .collect_vec();

        Self {
            coeffs: selected.try_into().unwrap(),
        }
    }

    pub fn constant(builder: &mut CircuitBuilder<F, D>, c: Fq12) -> Self {
        let c: MyFq12 = c.into();
        let coeffs = c
            .coeffs
            .iter()
            .map(|x| FqTarget::constant(builder, x.clone()))
            .collect_vec()
            .try_into()
            .unwrap();
        Self { coeffs }
    }

    pub fn add(&self, builder: &mut CircuitBuilder<F, D>, rhs: &Self) -> Self {
        let coeffs = self
            .coeffs
            .iter()
            .enumerate()
            .map(|(i, x)| x.add(builder, &rhs.coeffs[i]))
            .collect_vec()
            .try_into()
            .unwrap();
        Fq12Target { coeffs }
    }

    pub fn neg(&self, builder: &mut CircuitBuilder<F, D>) -> Self {
        let coeffs = self
            .coeffs
            .iter()
            .map(|x| x.neg(builder))
            .collect_vec()
            .try_into()
            .unwrap();
        Fq12Target { coeffs }
    }

    pub fn sub(&self, builder: &mut CircuitBuilder<F, D>, rhs: &Self) -> Self {
        let coeffs = self
            .coeffs
            .iter()
            .enumerate()
            .map(|(i, x)| x.sub(builder, &rhs.coeffs[i]))
            .collect_vec()
            .try_into()
            .unwrap();
        Fq12Target { coeffs }
    }

    pub fn mul(&self, builder: &mut CircuitBuilder<F, D>, rhs: &Self) -> Self {
        let a = self;
        let b = rhs;
        let mut a0b0_coeffs: Vec<FqTarget<F, D>> = Vec::with_capacity(11);
        let mut a0b1_coeffs: Vec<FqTarget<F, D>> = Vec::with_capacity(11);
        let mut a1b0_coeffs: Vec<FqTarget<F, D>> = Vec::with_capacity(11);
        let mut a1b1_coeffs: Vec<FqTarget<F, D>> = Vec::with_capacity(11);
        for i in 0..6 {
            for j in 0..6 {
                let coeff00 = a.coeffs[i].mul(builder, &b.coeffs[j]);
                let coeff01 = a.coeffs[i].mul(builder, &b.coeffs[j + 6]);
                let coeff10 = a.coeffs[i + 6].mul(builder, &b.coeffs[j]);
                let coeff11 = a.coeffs[i + 6].mul(builder, &b.coeffs[j + 6]);
                if i + j < a0b0_coeffs.len() {
                    a0b0_coeffs[i + j] = a0b0_coeffs[i + j].add(builder, &coeff00);
                    a0b1_coeffs[i + j] = a0b1_coeffs[i + j].add(builder, &coeff01);
                    a1b0_coeffs[i + j] = a1b0_coeffs[i + j].add(builder, &coeff10);
                    a1b1_coeffs[i + j] = a1b1_coeffs[i + j].add(builder, &coeff11);
                } else {
                    a0b0_coeffs.push(coeff00);
                    a0b1_coeffs.push(coeff01);
                    a1b0_coeffs.push(coeff10);
                    a1b1_coeffs.push(coeff11);
                }
            }
        }

        let mut a0b0_minus_a1b1: Vec<FqTarget<F, D>> = Vec::with_capacity(11);
        let mut a0b1_plus_a1b0: Vec<FqTarget<F, D>> = Vec::with_capacity(11);
        for i in 0..11 {
            let a0b0_minus_a1b1_entry = a0b0_coeffs[i].sub(builder, &a1b1_coeffs[i]);
            let a0b1_plus_a1b0_entry = a0b1_coeffs[i].add(builder, &a1b0_coeffs[i]);
            a0b0_minus_a1b1.push(a0b0_minus_a1b1_entry);
            a0b1_plus_a1b0.push(a0b1_plus_a1b0_entry);
        }

        let const_nine = FqTarget::constant(builder, Fq::from(9));
        let mut out_coeffs: Vec<FqTarget<F, D>> = Vec::with_capacity(12);
        for i in 0..6 {
            if i < 5 {
                // let coeff: Fq = a0b0_minus_a1b1[i] + Fq::from(9) * a0b0_minus_a1b1[i + 6]
                //     - a0b1_plus_a1b0[i + 6];
                let term0 = a0b0_minus_a1b1[i].clone();
                let term1 = a0b0_minus_a1b1[i + 6].mul(builder, &const_nine);
                let term2 = a0b1_plus_a1b0[i + 6].neg(builder);
                let term0_plus_term1 = term0.add(builder, &term1);
                let coeff = term0_plus_term1.add(builder, &term2);
                out_coeffs.push(coeff);
            } else {
                out_coeffs.push(a0b0_minus_a1b1[i].clone());
            }
        }
        for i in 0..6 {
            if i < 5 {
                // let coeff: Fq = a0b1_plus_a1b0[i]
                //     + a0b0_minus_a1b1[i + 6]
                //     + Fq::from(9) * a0b1_plus_a1b0[i + 6];
                let term0 = a0b1_plus_a1b0[i].clone();
                let term1 = a0b0_minus_a1b1[i + 6].clone();
                let term2 = a0b1_plus_a1b0[i + 6].mul(builder, &const_nine);
                let term0_plus_term1 = term0.add(builder, &term1);
                let coeff = term0_plus_term1.add(builder, &term2);
                out_coeffs.push(coeff);
            } else {
                out_coeffs.push(a0b1_plus_a1b0[i].clone());
            }
        }
        Self {
            coeffs: out_coeffs.try_into().unwrap(),
        }
    }

    pub fn pow(&self, builder: &mut CircuitBuilder<F, D>, offset: &Self, exp_val: Target) -> Self {
        let pow = Self::empty(builder);
        builder.add_simple_generator(Fq12ExpGenerator::<F, D> {
            x: self.clone(),
            offset: offset.clone(),
            exp_val,
            output: pow.clone(),
        });
        pow
    }

    pub fn div(&self, builder: &mut CircuitBuilder<F, D>, other: &Self) -> Self {
        let inv = other.inv(builder);
        self.mul(builder, &inv)
    }

    pub fn inv(&self, builder: &mut CircuitBuilder<F, D>) -> Self {
        let inv = Self::empty(builder);
        builder.add_simple_generator(Fq12InverseGenerator::<F, D> {
            x: self.clone(),
            inv: inv.clone(),
        });
        let one = Self::constant(builder, Fq12::ONE);
        let x_mul_inv = self.mul(builder, &inv);
        Self::connect(builder, &x_mul_inv, &one);
        inv
    }

    pub fn conjugate(&self, builder: &mut CircuitBuilder<F, D>) -> Self {
        let mut coeffs = self.coeffs.clone();
        coeffs[1] = coeffs[1].neg(builder);
        coeffs[3] = coeffs[3].neg(builder);
        coeffs[5] = coeffs[5].neg(builder);
        coeffs[7] = coeffs[7].neg(builder);
        coeffs[9] = coeffs[9].neg(builder);
        coeffs[11] = coeffs[11].neg(builder);
        Self { coeffs }
    }

    pub fn conditional_mul(
        &self,
        builder: &mut CircuitBuilder<F, D>,
        x: &Self,
        flag: &BoolTarget,
    ) -> Self {
        let muled = self.mul(builder, x);
        Self::select(builder, &muled, &self, flag)
    }

    pub fn serialize(
        &self,
        dst: &mut Vec<u8>,
        common_data: &CommonCircuitData<F, D>,
    ) -> Result<(), IoError> {
        for fq in &self.coeffs {
            fq.serialize(dst, common_data)?;
        }

        Ok(())
    }

    pub fn deserialize(
        src: &mut Buffer,
        common_data: &CommonCircuitData<F, D>,
    ) -> Result<Self, IoError> {
        let coeffs = [0; 12]
            .iter()
            .map(|_| FqTarget::deserialize(src, common_data))
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .unwrap();

        Ok(Self { coeffs })
    }
}

#[derive(Debug, Default)]
pub struct Fq12InverseGenerator<F: RichField + Extendable<D>, const D: usize> {
    x: Fq12Target<F, D>,
    inv: Fq12Target<F, D>,
}

impl<F: RichField + Extendable<D>, const D: usize> SimpleGenerator<F, D>
    for Fq12InverseGenerator<F, D>
{
    fn dependencies(&self) -> Vec<Target> {
        self.x
            .coeffs
            .iter()
            .flat_map(|coeff| coeff.target.value.limbs.iter().map(|&l| l.0))
            .collect_vec()
    }

    fn run_once(&self, witness: &PartitionWitness<F>, out_buffer: &mut GeneratedValues<F>) {
        let coeffs: Vec<Fq> = self
            .x
            .coeffs
            .iter()
            .map(|x| from_biguint_to_fq(witness.get_biguint_target(x.target.value.clone())))
            .collect_vec();
        let x = MyFq12 {
            coeffs: coeffs.try_into().unwrap(),
        };
        let x: Fq12 = x.into();
        let inv_x: Fq12 = x.inverse().unwrap();
        let inv_x: MyFq12 = inv_x.into();
        let inv_x_biguint: Vec<BigUint> = inv_x
            .coeffs
            .iter()
            .cloned()
            .map(|coeff| coeff.into())
            .collect_vec();

        for i in 0..12 {
            out_buffer.set_biguint_target(&self.inv.coeffs[i].target.value, &inv_x_biguint[i]);
        }
    }

    fn id(&self) -> std::string::String {
        "Fq12InverseGenerator".to_string()
    }

    fn serialize(
        &self,
        dst: &mut Vec<u8>,
        common_data: &CommonCircuitData<F, D>,
    ) -> Result<(), IoError> {
        self.x.serialize(dst, common_data)?;
        self.inv.serialize(dst, common_data)
    }

    fn deserialize(
        src: &mut Buffer,
        common_data: &CommonCircuitData<F, D>,
    ) -> Result<Self, IoError> {
        let x = Fq12Target::deserialize(src, common_data)?;
        let inv = Fq12Target::deserialize(src, common_data)?;

        Ok(Self { x, inv })
    }
}

#[derive(Debug, Default)]
pub struct Fq12ExpGenerator<F: RichField + Extendable<D>, const D: usize> {
    pub x: Fq12Target<F, D>,
    pub offset: Fq12Target<F, D>,
    pub exp_val: Target,
    pub output: Fq12Target<F, D>,
}

impl<F: RichField + Extendable<D>, const D: usize> SimpleGenerator<F, D>
    for Fq12ExpGenerator<F, D>
{
    fn dependencies(&self) -> Vec<Target> {
        self.x
            .coeffs
            .iter()
            .flat_map(|coeff| coeff.target.value.limbs.iter().map(|&l| l.0))
            .collect_vec()
    }

    fn run_once(&self, witness: &PartitionWitness<F>, out_buffer: &mut GeneratedValues<F>) {
        let x_coeffs: [Fq; 12] = self
            .x
            .clone()
            .coeffs
            .map(|x| get_u256_biguint(witness, &x.to_vec()).into());
        let x: Fq12 = MyFq12 { coeffs: x_coeffs }.into();
        let offset_coeffs = self
            .offset
            .clone()
            .coeffs
            .map(|x| get_u256_biguint(witness, &x.to_vec()).into());
        let offset: Fq12 = MyFq12 {
            coeffs: offset_coeffs,
        }
        .into();
        let exp_val = witness.get_target(self.exp_val).to_canonical_u64();
        let output = offset * x.pow(&[exp_val]);
        self.output.set_witness(out_buffer, &output);
    }

    fn id(&self) -> String {
        "Fq12ExpGenerator".to_string()
    }

    fn serialize(
        &self,
        dst: &mut Vec<u8>,
        common_data: &CommonCircuitData<F, D>,
    ) -> plonky2::util::serialization::IoResult<()> {
        self.x.serialize(dst, common_data)?;
        self.offset.serialize(dst, common_data)?;
        self.output.serialize(dst, common_data)?;
        dst.write_target(self.exp_val)
    }

    fn deserialize(
        src: &mut Buffer,
        common_data: &CommonCircuitData<F, D>,
    ) -> plonky2::util::serialization::IoResult<Self>
    where
        Self: Sized,
    {
        let x = Fq12Target::deserialize(src, common_data)?;
        let offset = Fq12Target::deserialize(src, common_data)?;
        let output = Fq12Target::deserialize(src, common_data)?;
        let exp_val = src.read_target()?;

        Ok(Self {
            x,
            offset,
            output,
            exp_val,
        })
    }
}

impl<F: RichField + Extendable<D>, const D: usize> Fq12Target<F, D> {
    pub fn to_vec(&self) -> Vec<Target> {
        self.coeffs.iter().flat_map(|c| c.to_vec()).collect()
    }

    pub fn from_vec(builder: &mut CircuitBuilder<F, D>, input: &[Target]) -> Self {
        let num_limbs = 8;
        assert_eq!(input.len(), 12 * num_limbs);
        let coeffs = input
            .iter()
            .cloned()
            .chunks(num_limbs)
            .into_iter()
            .map(|chunk| FqTarget::from_vec(builder, &chunk.collect_vec()))
            .collect_vec();
        Fq12Target {
            coeffs: coeffs.try_into().unwrap(),
        }
    }

    pub fn set_witness<W: WitnessWrite<F>>(&self, pw: &mut W, value: &Fq12) {
        let my_value: MyFq12 = value.clone().into();
        self.coeffs
            .iter()
            .cloned()
            .zip(my_value.coeffs)
            .map(|(c_t, c)| c_t.set_witness(pw, &c))
            .for_each(drop);
    }
}

#[cfg(test)]
mod tests {
    use ark_bn254::{Fq, Fq12};
    use ark_ff::Field;
    use ark_std::UniformRand;
    use num_bigint::BigUint;
    use plonky2::{
        field::{goldilocks_field::GoldilocksField, types::Field as Plonky2Field},
        iop::{target::Target, witness::PartialWitness},
        plonk::{
            circuit_builder::CircuitBuilder, circuit_data::CircuitConfig,
            config::PoseidonGoldilocksConfig,
        },
    };
    use rand::Rng;

    use super::{from_biguint_to_fq, Fq12Target};

    type F = GoldilocksField;
    type C = PoseidonGoldilocksConfig;
    const D: usize = 2;

    #[test]
    fn test_from_to_vec() {
        let rng = &mut rand::thread_rng();
        let a = Fq12::rand(rng);
        let config = CircuitConfig::standard_ecc_config();
        let mut builder = CircuitBuilder::<F, D>::new(config);
        let a_t = Fq12Target::constant(&mut builder, a);

        let a_vec = a_t.to_vec();
        let restored_a_t = Fq12Target::from_vec(&mut builder, &a_vec);

        Fq12Target::connect(&mut builder, &a_t, &restored_a_t);

        let pw = PartialWitness::new();
        let data = builder.build::<C>();
        let _proof = data.prove(pw);
    }

    #[test]
    fn test_from_biguint_to_fq() {
        let rng = &mut rand::thread_rng();
        let x = Fq::rand(rng);
        let x_biguint: BigUint = x.into();
        let converted_x = from_biguint_to_fq(x_biguint);
        assert_eq!(x, converted_x);
    }

    #[test]
    fn test_fq12_mul_circuit() {
        let rng = &mut rand::thread_rng();
        let a = Fq12::rand(rng);
        let b = Fq12::rand(rng);
        let c_expected = a * b;

        let config = CircuitConfig::standard_ecc_config();
        let mut builder = CircuitBuilder::<F, D>::new(config);
        let a_t = Fq12Target::constant(&mut builder, a);
        let b_t = Fq12Target::constant(&mut builder, b);
        let c_t = a_t.mul(&mut builder, &b_t);
        let c_expected_t = Fq12Target::constant(&mut builder, c_expected);

        Fq12Target::connect(&mut builder, &c_expected_t, &c_t);

        let pw = PartialWitness::new();
        let data = builder.build::<C>();
        let _proof = data.prove(pw);
    }

    #[test]
    fn test_fq12_inv_circuit() {
        let rng = &mut rand::thread_rng();
        let x: Fq12 = Fq12::rand(rng);
        let inv_x_expected = x.inverse().unwrap();

        let config = CircuitConfig::standard_ecc_config();
        let mut builder = CircuitBuilder::<F, D>::new(config);
        let x_t = Fq12Target::constant(&mut builder, x);
        let inv_x_t = x_t.inv(&mut builder);
        let inv_x_expected_t = Fq12Target::constant(&mut builder, inv_x_expected);

        Fq12Target::connect(&mut builder, &inv_x_t, &inv_x_expected_t);

        let pw = PartialWitness::new();
        let data = builder.build::<C>();
        dbg!(data.common.degree_bits());
        let _proof = data.prove(pw);
    }

    #[test]
    fn test_fq12_pow_circuit() {
        let rng = &mut rand::thread_rng();
        let x: Fq12 = Fq12::rand(rng);
        let exp: u64 = rng.gen();
        let pow_expected = x.pow([exp]);

        let config = CircuitConfig::standard_ecc_config();
        let mut builder = CircuitBuilder::<F, D>::new(config);
        let x_t = Fq12Target::constant(&mut builder, x);
        let offset = Fq12Target::constant(&mut builder, Fq12::ONE);
        let exp_val = builder.constant(F::from_canonical_u64(exp));
        let pow_x_t = x_t.pow(&mut builder, &offset, exp_val);
        let pow_x_expected_t = Fq12Target::constant(&mut builder, pow_expected);

        Fq12Target::connect(&mut builder, &pow_x_t, &pow_x_expected_t);

        let pw = PartialWitness::new();
        let data = builder.build::<C>();
        dbg!(data.common.degree_bits());
        let _proof = data.prove(pw);
    }
}
