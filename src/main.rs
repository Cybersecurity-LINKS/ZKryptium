use std::{time::Instant, any::TypeId};

use bls12_381_plus::{G1Affine, G2Affine, pairing, G1Projective, Scalar};
use byteorder::BigEndian;
use digest::Digest;
use elliptic_curve::hash2curve::ExpandMsg;
use glass_pumpkin::prime::new;
use hex::ToHex;
use links_crypto::{utils::random, keys::{cl03_key::{CL03PublicKey, CL03CommitmentPublicKey}, pair::{KeyPair}, bbsplus_key::{BBSplusSecretKey, BBSplusPublicKey}, key::PublicKey}, bbsplus::{generators::{make_generators, global_generators, signer_specific_generators, print_generators}, ciphersuites::{Bls12381Shake256, BbsCiphersuite, Bls12381Sha256}, message::{Message, BBSplusMessage, CL03Message}}, schemes::algorithms::{CL03, BBSplus, Scheme, CL03Sha256, BBSplusShake256, BBSplusSha256, Ciphersuite}, signatures::{commitment::{Commitment, BBSplusCommitment, self}, blind::{self, BlindSignature, BBSplusBlindSignature}, signature::{BBSplusSignature, Signature, self}, proof::{PoKSignature, CL03PoKSignature, NISPSignaturePoK, ZKPoK}}, cl03::ciphersuites::{CLSha256, CLCiphersuite}};

use links_crypto::keys::key::PrivateKey;
use rug::{Integer, Complete};

fn bbsplus_main<S: Scheme>() 
where
    S::Ciphersuite: BbsCiphersuite,
    <S::Ciphersuite as BbsCiphersuite>::Expander: for<'a> ExpandMsg<'a>,
{
    println!("\nRunnig BBSplus signature algorithm...\n");

    const IKM: &str = "746869732d49532d6a7573742d616e2d546573742d494b4d2d746f2d67656e65726174652d246528724074232d6b6579";
    const KEY_INFO: &str = "746869732d49532d736f6d652d6b65792d6d657461646174612d746f2d62652d757365642d696e2d746573742d6b65792d67656e";
    const msgs: [&str; 3] = ["9872ad089e452c7b6e283dfac2a80d58e8d0ff71cc4d5e310a1debdda4a45f02", "87a8bd656d49ee07b8110e1d8fd4f1dcef6fb9bc368c492d9bc8c4f98a739ac6", "96012096adda3f13dd4adbe4eea481a4c4b5717932b73b00e31807d3c5894b90"];
    const header_hex: &str = "11223344556677889900aabbccddeeff";
    let dst: Vec<u8> =  hex::decode("4242535f424c53313233383147315f584d443a5348412d3235365f535357555f524f5f4d41505f4d53475f544f5f5343414c41525f41535f484153485f").unwrap();
    let header = hex::decode(header_hex).unwrap();
    let unrevealed_message_indexes = [1usize];
    let revealed_message_indexes = [0usize, 2usize];
    // let nonce = generate_nonce();
    let nonce = b"aaaa".as_slice();

    log::info!("Keypair Generation");
    let issuer_keypair = KeyPair::<BBSplus<S::Ciphersuite>>::generate(
        &hex::decode(&IKM).unwrap(),
        Some(&hex::decode(&KEY_INFO).unwrap())
    );


    let issuer_sk = issuer_keypair.private_key();
    let issuer_pk = issuer_keypair.public_key();

    log::info!("Computing Generators");
    let get_generators_fn = make_generators::<<S as Scheme>::Ciphersuite>;
    let generators = global_generators(get_generators_fn, msgs.len() + 2);

    //Map Messages to Scalars

    let msgs_scalars: Vec<BBSplusMessage> = msgs.iter().map(|m| BBSplusMessage::map_message_to_scalar_as_hash::<S::Ciphersuite>(&hex::decode(m).unwrap(), Some(&dst))).collect();
    
    log::info!("Computing pedersen commitment on messages");
    let commitment = Commitment::<BBSplus<S::Ciphersuite>>::commit(&msgs_scalars, Some(&generators), &unrevealed_message_indexes);
    
    
    let unrevealed_msgs: Vec<BBSplusMessage> = msgs_scalars.iter().enumerate().filter_map(|(i, m)| {
        if unrevealed_message_indexes.contains(&i) {
            Some(*m)
        } else {
            None
        }
    }).collect();

    let revealed_msgs: Vec<BBSplusMessage> = msgs_scalars.iter().enumerate().filter_map(|(i, m)| {
        if !unrevealed_message_indexes.contains(&i) {
            Some(*m)
        } else {
            None
        }
    }).collect();


    log::info!("Computation of a Zero-Knowledge proof-of-knowledge of committed messages");
    let zkpok = ZKPoK::<BBSplus<S::Ciphersuite>>::generate_proof(&unrevealed_msgs, commitment.bbsPlusCommitment(), &generators, &unrevealed_message_indexes, &nonce);


    //Issuer compute blind signature
    log::info!("Verification of the Zero-Knowledge proof and computation of a blind signature");
    let blind_signature = BlindSignature::<BBSplus<S::Ciphersuite>>::blind_sign(&revealed_msgs, commitment.bbsPlusCommitment(), &zkpok, issuer_sk, issuer_pk, &generators, &revealed_message_indexes, &unrevealed_message_indexes, &nonce, Some(&header));

    if let Err(e) = &blind_signature {
        println!("Error: {}", e);
    }
    
    assert!(blind_signature.is_ok(), "Blind Signature Error");

    //Holder unblind the signature
    log::info!("Signature unblinding and verification...");
    let unblind_signature = blind_signature.unwrap().unblind_sign(commitment.bbsPlusCommitment());

    let verify = unblind_signature.verify(issuer_pk, Some(&msgs_scalars), &generators, Some(&header));

    assert!(verify, "Unblinded Signature NOT VALID!");
    log::info!("Signature is VALID!");

    //Holder generates SPoK
    log::info!("Computation of a Zero-Knowledge proof-of-knowledge of a signature");
    let ph = hex::decode("bed231d880675ed101ead304512e043ade9958dd0241ea70b4b3957fba941501").unwrap();
    let proof = PoKSignature::<BBSplus<S::Ciphersuite>>::proof_gen(unblind_signature.bbsPlusSignature(), &issuer_pk, Some(&msgs_scalars), &generators, Some(&revealed_message_indexes), Some(&header), Some(&ph), None);

    //Verifier verifies SPok
    log::info!("Signature Proof of Knowledge verification...");
    let proof_result = proof.proof_verify(&issuer_pk, Some(&revealed_msgs), &generators, Some(&revealed_message_indexes), Some(&header), Some(&ph));
    assert!(proof_result, "Signature Proof of Knowledge Verification Failed!");
    log::info!("Signature Proof of Knowledge is VALID!");

}


fn cl03_main<S: Scheme>() 
where
    S::Ciphersuite: CLCiphersuite,
    <S::Ciphersuite as Ciphersuite>::HashAlg: Digest
{
    println!("\nRunnig CL2003 ignature algorithm...\n");

    const msgs: &[&str] = &["9872ad089e452c7b6e283dfac2a80d58e8d0ff71cc4d5e310a1debdda4a45f02", "9872ad089e452c7b6e283dfac2a80d58e8d0ff71cc4d5e310a1debdda4a45f03", "9872ad089e452c7b6e283dfac2a80d58e8d0ff71cc4d5e310a1debdda4a45f04"];
    
    log::info!("Keypair Generation");
    let issuer_keypair = KeyPair::<CL03<S::Ciphersuite>>::generate(Some(msgs.len().try_into().unwrap()));
    
    let messages: Vec<CL03Message> = msgs.iter().map(|&m| CL03Message::map_message_to_integer_as_hash::<S::Ciphersuite>(&hex::decode(m).unwrap()) ).collect();
    
 
    let unrevealed_message_indexes = [0usize];
    let revealed_message_indexes = [1usize,  2usize];
    let revealed_messages: Vec<CL03Message> = messages.iter().enumerate().filter(|&(i,_)| revealed_message_indexes.contains(&i) ).map(|(_, m)| m.clone()).collect();
    
    log::info!("Computing pedersen commitment on messages");
    let commitment = Commitment::<CL03<S::Ciphersuite>>::commit_with_pk(&messages, issuer_keypair.public_key(), Some(&unrevealed_message_indexes));
    
    log::info!("Computation of a Zero-Knowledge proof-of-knowledge of committed messages");
    let zkpok = ZKPoK::<CL03<S::Ciphersuite>>::generate_proof(&messages, commitment.cl03Commitment(), None, issuer_keypair.public_key(), None, &unrevealed_message_indexes);

    log::info!("Verification of the Zero-Knowledge proof and computation of a blind signature");
    let blind_signature = BlindSignature::<CL03<S::Ciphersuite>>::blind_sign(issuer_keypair.public_key(), issuer_keypair.private_key(), &zkpok, Some(&revealed_messages), commitment.cl03Commitment(), None, None, &unrevealed_message_indexes, Some(&revealed_message_indexes));
    
    log::info!("Signature unblinding and verification...");
    let unblided_signature = blind_signature.unblind_sign(&commitment);
    let verify = unblided_signature.verify_multiattr(issuer_keypair.public_key(), &messages);

    assert!(verify, "Error! The unblided signature verification should PASS!");
    log::info!("Signature is VALID!");

    //Verifier generates its pk
    log::info!("Generation of a Commitment Public Key for the computation of the SPoK");
    let verifier_commitment_pk = CL03CommitmentPublicKey::generate::<S::Ciphersuite>(Some(issuer_keypair.public_key().N.clone()), Some(msgs.len()));

    //Holder compute the Signature Proof of Knowledge
    log::info!("Computation of a Zero-Knowledge proof-of-knowledge of a signature");
    let signature_pok = PoKSignature::<CL03<S::Ciphersuite>>::proof_gen(unblided_signature.cl03Signature(), &verifier_commitment_pk, issuer_keypair.public_key(), &messages, &unrevealed_message_indexes);
    
    //Verifier verifies the Signature Proof of Knowledge
    log::info!("Signature Proof of Knowledge verification...");
    let valid_proof = signature_pok.proof_verify(&verifier_commitment_pk, issuer_keypair.public_key(), &revealed_messages, &unrevealed_message_indexes, msgs.len());
    
    assert!(valid_proof, "Error! The signature proof of knowledge should PASS!");
    log::info!("Signature Proof of Knowledge is VALID!");
}


fn test_bbsplus_sign() {

    const IKM: &str = "746869732d49532d6a7573742d616e2d546573742d494b4d2d746f2d67656e65726174652d246528724074232d6b6579";
    const KEY_INFO: &str = "746869732d49532d736f6d652d6b65792d6d657461646174612d746f2d62652d757365642d696e2d746573742d6b65792d67656e";



    const msg: &str = "9872ad089e452c7b6e283dfac2a80d58e8d0ff71cc4d5e310a1debdda4a45f02";
    // const msg: &str = "c344136d9ab02da4dd5908bbba913ae6f58c2cc844b802a6f811f5fb075f9b80";
    const msg_wrong: &str = "9872ad089e452c7b6e283dfac2a80d58e8d0ff71cc4d5e310a1debdda4a45f03";
    const dst: &str = "4242535f424c53313233383147315f584f463a5348414b452d3235365f535357555f524f5f4d41505f4d53475f544f5f5343414c41525f41535f484153485f";
    const dst_sha256: &str = "4242535f424c53313233383147315f584d443a5348412d3235365f535357555f524f5f4d41505f4d53475f544f5f5343414c41525f41535f484153485f";
    const header:&str = "11223344556677889900aabbccddeeff";
    const ph: &str = "bed231d880675ed101ead304512e043ade9958dd0241ea70b4b3957fba941501";
    let revealed_message_indexes = [0usize];
    const seed: &str = "332e313431353932363533353839373933323338343632363433333833323739";
                        

    let bbsplus_keypair = KeyPair::<BBSplusSha256>::generate(
        &hex::decode(&IKM).unwrap(),
        Some(&hex::decode(&KEY_INFO).unwrap())
    );

    println!("PK {}", hex::encode(bbsplus_keypair.public_key().to_bytes()));

    let get_generators_fn = make_generators::<<BBSplusSha256 as Scheme>::Ciphersuite>;

    let generators = global_generators(get_generators_fn, 3);
    print_generators(&generators);

    let message = BBSplusMessage::map_message_to_scalar_as_hash::<Bls12381Sha256>(&hex::decode(msg).unwrap(), Some(&hex::decode(dst_sha256).unwrap()));
    let message_bytes = message.to_bytes_be();
    println!("message: {:?}", hex::encode(message_bytes));

    let mut messages: Vec<BBSplusMessage> = Vec::new();
    messages.push(message);

    println!("scalars: {:?}", messages);


    let message_to_verify = BBSplusMessage::map_message_to_scalar_as_hash::<Bls12381Sha256>(&hex::decode(msg_wrong).unwrap(), Some(&hex::decode(dst_sha256).unwrap()));
    let mut messages_to_verify: Vec<BBSplusMessage> = Vec::new();
    messages_to_verify.push(message_to_verify);

    let signature = Signature::<BBSplusSha256>::sign(Some(&messages), bbsplus_keypair.private_key(), bbsplus_keypair.public_key(), &generators, Some(&hex::decode(header).unwrap()));
    let enc = hex::encode(signature.to_bytes());
    println!("signature: {}", hex::encode(signature.to_bytes()));
    println!("signature: {:?}", signature);
    let signature2 = Signature::<BBSplusSha256>::from_bytes(hex::decode(enc).unwrap().as_slice().try_into().unwrap()).unwrap();
    println!("signature2: {:?}", signature2);
    let valid = signature.verify(bbsplus_keypair.public_key(), Some(&messages_to_verify), &generators, Some(&hex::decode(header).unwrap()));
    println!("{}", valid);

    // let signature_PoK = PoKSignature::<BBSplusSha256>::proof_gen(signature.bbsPlusSignature(), bbsplus_keypair.public_key(), Some(&messages), &generators, Some(&revealed_message_indexes), Some(&hex::decode(header).unwrap()), Some(&hex::decode(ph).unwrap()), Some(&hex::decode(seed).unwrap()));
    // println!("SPoK: {}", hex::encode(signature_PoK.to_bytes()));

    let signature_PoK2 = PoKSignature::<BBSplusSha256>::proof_gen(signature2.bbsPlusSignature(), bbsplus_keypair.public_key(), Some(&messages), &generators, Some(&revealed_message_indexes), Some(&hex::decode(header).unwrap()), Some(&hex::decode(ph).unwrap()), Some(&hex::decode(seed).unwrap()));
    println!("SKPOK2: {:?}", signature_PoK2);
    let enc_proof = hex::encode(signature_PoK2.to_bytes());
    println!("SPoK2: {}", enc_proof);

    let unwrap_proof = PoKSignature::<BBSplusSha256>::from_bytes(&hex::decode(enc_proof).unwrap());
    

    // let valid = signature_PoK.proof_verify(bbsplus_keypair.public_key(), Some(&messages), &generators, Some(&revealed_message_indexes), Some(&hex::decode(header).unwrap()), Some(&hex::decode(ph).unwrap()));
    // println!("SPoK verify: {}", valid);
}


fn test_cl03_sign() {
    const msg: &str = "9872ad089e452c7b6e283dfac2a80d58e8d0ff71cc4d5e310a1debdda4a45f02";
    const msg2: &str = "9872ad089e452c7b6e283dfac2a80d58e8d0ff71cc4d5e310a1debdda4a45f03";
    const wrong_msg: &str = "9872ad089e452c7b6e283dfac2a80d58e8d0ff71cc4d5e310a1debdda4a45f03";

    let cl03_keypair = KeyPair::<CL03Sha256>::generate(Some(2));
    let commitment_pk = CL03CommitmentPublicKey::generate::<CLSha256>(Some(cl03_keypair.public_key().N.clone()), Some(2));

    let message = CL03Message::map_message_to_integer_as_hash::<CLSha256>(&hex::decode(msg).unwrap());
    let message2 = CL03Message::map_message_to_integer_as_hash::<CLSha256>(&hex::decode(msg2).unwrap());
    let messages = [message.clone()];
    let unrevealed_message_indexes = [0usize];
    let wrong_message = CL03Message::map_message_to_integer_as_hash::<CLSha256>(&hex::decode(wrong_msg).unwrap());

    
    let signature = Signature::<CL03Sha256>::sign(cl03_keypair.public_key(), cl03_keypair.private_key(), &message);

    let bytes = signature.to_bytes();
    // println!("\n signature: {}", hex::encode(&bytes));

    let signature_copy = Signature::<CL03Sha256>::from_bytes(&bytes);

    // println!("\n signature {}", hex::encode(&signature_copy.to_bytes()));

    // println!("compare: {}", signature == signature_copy);

    let valid = signature.verify(cl03_keypair.public_key(), &message);

    println!("valid: {}", valid);

    let valid2 = signature.verify_multiattr(cl03_keypair.public_key(), &[message]);

    println!("valid multiattr: {}", valid2);

    
    let commitment = Commitment::<CL03Sha256>::commit_with_pk(&messages, cl03_keypair.public_key(), Some(&unrevealed_message_indexes));
    
    let zkpok = ZKPoK::<CL03Sha256>::generate_proof(&messages, commitment.cl03Commitment(), None, cl03_keypair.public_key(), None, &unrevealed_message_indexes);

    let blind_signature = BlindSignature::<CL03Sha256>::blind_sign(cl03_keypair.public_key(), cl03_keypair.private_key(), &zkpok, None, commitment.cl03Commitment(), None, None, &unrevealed_message_indexes, None);
    let unblided_signature = blind_signature.unblind_sign(&commitment);
    let verify = unblided_signature.verify_multiattr(cl03_keypair.public_key(), &messages);

    println!("valid signature multimessage: {}", verify);


    let signature_pok = NISPSignaturePoK::nisp5_MultiAttr_generate_proof::<CLSha256>(unblided_signature.cl03Signature(), &commitment_pk, cl03_keypair.public_key(), &messages, &unrevealed_message_indexes);

    let valid_proof = signature_pok.nisp5_MultiAttr_verify_proof::<CLSha256>(&commitment_pk, cl03_keypair.public_key(), &messages, &unrevealed_message_indexes, 1);
    println!("valid proof: {}", valid_proof);

}


//Cannot be done!
// fn test<PK: PublicKey>(pk: &PK){
//     let (N, b, c, a_bases) = pk.get_params();
// }


fn main() {
    dotenv::dotenv().ok();
    env_logger::init();

    bbsplus_main::<BBSplusSha256>();
    cl03_main::<CL03Sha256>();


    // test_bbsplus_sign();
    // test_cl03_sign();

    // // let pair_cs =  KeyPair::<CL03Sha256>::generate(Some(2));
    // // test(pair_cs.public_key());

    // const IKM: &str = "746869732d49532d6a7573742d616e2d546573742d494b4d2d746f2d67656e65726174652d246528724074232d6b6579";
    // const KEY_INFO: &str = "746869732d49532d736f6d652d6b65792d6d657461646174612d746f2d62652d757365642d696e2d746573742d6b65792d67656e";



    // let bbsplus_keypair = KeyPair::<BBSplusSha256>::generate(
    //     &hex::decode(&IKM).unwrap(),
    //     Some(&hex::decode(&KEY_INFO).unwrap())
    // );


    // let public = bbsplus_keypair.private_key();

    // println!("{}", public.encode());

    // let binding = public.to_bytes();
    // let bytes = binding.as_slice();

    // let pub2 = BBSplusSecretKey::from_bytes(bytes);

    // println!("{}", pub2.encode());

}