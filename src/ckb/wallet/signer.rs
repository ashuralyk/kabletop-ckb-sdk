use ckb_types::{
    bytes::Bytes, prelude::*, H256, core::TransactionView,
    packed::{
        self, WitnessArgs, Byte32
    }
};
use ckb_hash::new_blake2b;
use ckb_crypto::secp::Privkey;
use crate::ckb::transaction::helper;

// sign a whole [tx] with private [key], the [extra_witnesses] is some external args which just placed into witness part
pub async fn sign(tx: TransactionView, key: &Privkey, extra_witnesses: Vec<WitnessArgs>) -> TransactionView {
    let futures = tx
        .inputs()
        .into_iter()
        .map(|input| helper::outpoint_to_output(input.previous_output()))
        .collect::<Vec<_>>();
    let mut inputs = vec![];
    for input in futures {
        inputs.push(input.await.expect("get output before sign"));
    }
    let mut last_lockhash = Byte32::new([0u8; 32]);
    let mut signed_witnesses = inputs
        .iter()
        .enumerate()
        .map(|(i, input)| {
            if input.lock().calc_script_hash() == last_lockhash {
                Bytes::new().pack()
            } else {
                let witness = {
                    if let Some(witness) = tx.witnesses().get(i) {
                        WitnessArgs::new_unchecked(witness.unpack())
                    } else {
                        WitnessArgs::default()
                    }
                };
                last_lockhash = input.lock().calc_script_hash();
                sign_input(&tx, key, &witness, &extra_witnesses)
            }
        })
        .collect::<Vec<_>>();
    let mut extra_witnesses = extra_witnesses
        .iter()
        .map(|witness| witness.as_bytes().pack())
        .collect::<Vec<_>>();
    signed_witnesses.append(&mut extra_witnesses);
    return tx
        .as_advanced_builder()
        .set_witnesses(signed_witnesses)
        .build()
}

// sign the every single input data in [tx] and get the signed bytes
fn sign_input(
    tx: &TransactionView, key: &Privkey, witness: &WitnessArgs, extra_witnesses: &Vec<WitnessArgs>
) -> packed::Bytes {
    let tx_hash = tx.hash();
    let mut blake2b = new_blake2b();
    blake2b.update(&tx_hash.raw_data());
    let signed_witness = witness
        .clone()
        .as_builder()
        .lock(Some(Bytes::from(vec![0u8; 65])).pack())
        .build();
    let witness_len = signed_witness.as_bytes().len() as u64;
    blake2b.update(&witness_len.to_le_bytes());
    blake2b.update(&signed_witness.as_bytes());
    for extra_witness in extra_witnesses {
        let witness_len = witness.as_bytes().len() as u64;
        blake2b.update(&witness_len.to_le_bytes());
        blake2b.update(&extra_witness.as_bytes());
    }
    let mut digest = [0u8; 32];
    blake2b.finalize(&mut digest);
    let signature = key.sign_recoverable(&H256::from(digest)).expect("sign tx");
    signed_witness
        .as_builder()
        .lock(Some(Bytes::from(signature.serialize())).pack())
        .build()
        .as_bytes()
        .pack()
}
