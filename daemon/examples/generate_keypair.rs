// Run this ONCE to generate your keypair. Never commit the output.
// The private key goes into your secrets manager.
// The public key gets committed to the repo as keys/memix_public.der

fn main() {
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    let rng = SystemRandom::new();
    let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref()).unwrap();

    // This is what goes into MEMIX_PRIVATE_KEY env var on your server
    println!("MEMIX_PRIVATE_KEY={}", base64::encode(pkcs8_bytes.as_ref()));

    // This file gets committed to the repo — it is NOT a secret
    std::fs::create_dir_all("keys").unwrap();
    std::fs::write("keys/memix_public.der", pair.public_key().as_ref()).unwrap();
    println!("Public key written to keys/memix_public.der");
    println!("IMPORTANT: Copy MEMIX_PRIVATE_KEY to your secrets manager NOW.");
    println!("Do not save it to any file.");
}