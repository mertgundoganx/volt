//! NTLM, for the intranets that still ask for it.
//!
//! Two things make NTLM awkward, and both are written down rather than hidden.
//! It authenticates a *connection*, not a request, so the three legs of the
//! handshake have to go down the same socket — which is why `execute` builds a
//! dedicated HTTP/1.1 client with one pooled connection when it sees this
//! scheme. And the NT hash is MD4 of the password, which is broken and cannot
//! be anything else: this exists to talk to servers that already chose it.
//!
//! NTLMv2 only. LM and NTLMv1 responses are not produced, and a server that
//! accepts nothing else will simply refuse.

use hmac::{Hmac, KeyInit as _, Mac};
use md4::Md4;
use md5::Md5;
use sha2::Digest as _;

use crate::error::{Error, Result};

type HmacMd5 = Hmac<Md5>;

const SIGNATURE: &[u8; 8] = b"NTLMSSP\0";

/// The flags volt negotiates: Unicode, NTLM, always-sign, extended session
/// security, and "I will send a target name".
const FLAGS: u32 = 0x0000_0001 | 0x0000_0200 | 0x0000_8000 | 0x0008_0000 | 0x0000_0004;

/// What the server said in its challenge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub server_challenge: [u8; 8],
    /// The AV pairs, passed back verbatim inside the response blob.
    pub target_info: Vec<u8>,
    pub flags: u32,
}

/// The negotiate message, which says nothing except "NTLM, please".
pub fn negotiate() -> Vec<u8> {
    let mut out = Vec::with_capacity(32);
    out.extend_from_slice(SIGNATURE);
    out.extend_from_slice(&1u32.to_le_bytes()); // type 1
    out.extend_from_slice(&FLAGS.to_le_bytes());
    // Domain and workstation: empty, offset past the header. Sending them is
    // optional and saying nothing is the portable choice.
    out.extend_from_slice(&[0, 0, 0, 0, 32, 0, 0, 0]);
    out.extend_from_slice(&[0, 0, 0, 0, 32, 0, 0, 0]);
    out
}

pub fn parse_challenge(bytes: &[u8]) -> Result<Challenge> {
    if bytes.len() < 32 || &bytes[..8] != SIGNATURE {
        return Err(Error::Invalid("that is not an NTLM challenge".into()));
    }
    if u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) != 2 {
        return Err(Error::Invalid("the server sent an NTLM message that is not a challenge".into()));
    }

    let mut server_challenge = [0u8; 8];
    server_challenge.copy_from_slice(&bytes[24..32]);
    let flags = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);

    // The target info block is described by a length/offset pair at 40.
    let target_info = if bytes.len() >= 48 {
        let length = u16::from_le_bytes([bytes[40], bytes[41]]) as usize;
        let offset = u32::from_le_bytes([bytes[44], bytes[45], bytes[46], bytes[47]]) as usize;
        bytes.get(offset..offset + length).map(<[u8]>::to_vec).unwrap_or_default()
    } else {
        Vec::new()
    };

    Ok(Challenge { server_challenge, target_info, flags })
}

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn hmac_md5(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut mac = HmacMd5::new_from_slice(key).expect("HMAC takes a key of any length");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

/// NTLMv2: MD4 of the password, then HMAC-MD5 over the user and domain.
fn ntlmv2_hash(username: &str, password: &str, domain: &str) -> Vec<u8> {
    let nt = Md4::digest(utf16(password));
    hmac_md5(&nt, &utf16(&format!("{}{}", username.to_uppercase(), domain)))
}

/// 100-nanosecond intervals since 1601, which is how Windows counts.
fn filetime(now_seconds: u64) -> u64 {
    (now_seconds + 11_644_473_600) * 10_000_000
}

/// The authenticate message. `now` and `client_nonce` are parameters so a test
/// can pin them; in use they come from the clock and the random source.
pub fn authenticate(
    challenge: &Challenge,
    username: &str,
    password: &str,
    domain: &str,
    workstation: &str,
    now: u64,
    client_nonce: [u8; 8],
) -> Vec<u8> {
    let key = ntlmv2_hash(username, password, domain);

    // The blob: version, reserved, timestamp, our nonce, then the server's
    // target info, then four zero bytes.
    let mut blob = Vec::with_capacity(32 + challenge.target_info.len());
    blob.extend_from_slice(&[0x01, 0x01, 0, 0, 0, 0, 0, 0]);
    blob.extend_from_slice(&filetime(now).to_le_bytes());
    blob.extend_from_slice(&client_nonce);
    blob.extend_from_slice(&[0, 0, 0, 0]);
    blob.extend_from_slice(&challenge.target_info);
    blob.extend_from_slice(&[0, 0, 0, 0]);

    let mut proof_input = Vec::with_capacity(8 + blob.len());
    proof_input.extend_from_slice(&challenge.server_challenge);
    proof_input.extend_from_slice(&blob);
    let proof = hmac_md5(&key, &proof_input);

    let mut nt_response = proof;
    nt_response.extend_from_slice(&blob);

    // LM response: a v2 client that will not do LM sends zeroes.
    let lm_response = [0u8; 24];

    let domain_bytes = utf16(domain);
    let user_bytes = utf16(username);
    let workstation_bytes = utf16(workstation);
    let session_key: [u8; 0] = [];

    // Header is 64 bytes, then the payload in the order the fields describe.
    let mut offset = 64u32;
    let mut header = Vec::with_capacity(64);
    let mut payload = Vec::new();

    header.extend_from_slice(SIGNATURE);
    header.extend_from_slice(&3u32.to_le_bytes()); // type 3

    let field = |bytes: &[u8], header: &mut Vec<u8>, payload: &mut Vec<u8>, offset: &mut u32| {
        let length = bytes.len() as u16;
        header.extend_from_slice(&length.to_le_bytes());
        header.extend_from_slice(&length.to_le_bytes());
        header.extend_from_slice(&offset.to_le_bytes());
        payload.extend_from_slice(bytes);
        *offset += length as u32;
    };

    field(&lm_response, &mut header, &mut payload, &mut offset);
    field(&nt_response, &mut header, &mut payload, &mut offset);
    field(&domain_bytes, &mut header, &mut payload, &mut offset);
    field(&user_bytes, &mut header, &mut payload, &mut offset);
    field(&workstation_bytes, &mut header, &mut payload, &mut offset);
    field(&session_key, &mut header, &mut payload, &mut offset);
    header.extend_from_slice(&FLAGS.to_le_bytes());

    header.extend_from_slice(&payload);
    header
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_negotiate_message_says_ntlm_and_nothing_else() {
        let message = negotiate();
        assert_eq!(&message[..8], SIGNATURE);
        assert_eq!(u32::from_le_bytes([message[8], message[9], message[10], message[11]]), 1);
        assert_eq!(message.len(), 32, "header only: no domain, no workstation");
    }

    /// A challenge built by hand, so the offsets are read rather than trusted.
    fn challenge_bytes() -> Vec<u8> {
        let target_info = b"\x02\x00\x06\x00DOMAIN\x00\x00".to_vec();
        let mut message = Vec::new();
        message.extend_from_slice(SIGNATURE);
        message.extend_from_slice(&2u32.to_le_bytes());
        // Target name field, unused here.
        message.extend_from_slice(&[0, 0, 0, 0, 48, 0, 0, 0]);
        message.extend_from_slice(&0x0008_8201u32.to_le_bytes());
        message.extend_from_slice(b"\x01\x23\x45\x67\x89\xab\xcd\xef"); // server challenge
        message.extend_from_slice(&[0; 8]); // reserved
        let length = target_info.len() as u16;
        message.extend_from_slice(&length.to_le_bytes());
        message.extend_from_slice(&length.to_le_bytes());
        // 56: after the 8-byte version block that follows this header.
        message.extend_from_slice(&56u32.to_le_bytes());
        message.extend_from_slice(&[0; 8]); // version
        message.extend_from_slice(&target_info);
        message
    }

    #[test]
    fn a_challenge_is_read_including_the_block_it_has_to_hand_back() {
        let found = parse_challenge(&challenge_bytes()).expect("a challenge");

        assert_eq!(found.server_challenge, [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]);
        assert_eq!(found.target_info, b"\x02\x00\x06\x00DOMAIN\x00\x00".to_vec());
        assert_eq!(found.flags, 0x0008_8201);

        assert!(parse_challenge(b"nope").is_err());
        assert!(parse_challenge(&negotiate()).is_err(), "a negotiate message is not a challenge");
    }

    #[test]
    fn the_authenticate_message_carries_an_ntlmv2_response_and_no_lm_one() {
        let challenge = parse_challenge(&challenge_bytes()).unwrap();
        let message = authenticate(&challenge, "ada", "hunter2", "DOMAIN", "LAPTOP", 1_700_000_000, [7; 8]);

        assert_eq!(&message[..8], SIGNATURE);
        assert_eq!(u32::from_le_bytes([message[8], message[9], message[10], message[11]]), 3);

        let field = |at: usize| {
            let length = u16::from_le_bytes([message[at], message[at + 1]]) as usize;
            let offset = u32::from_le_bytes([message[at + 4], message[at + 5], message[at + 6], message[at + 7]])
                as usize;
            message[offset..offset + length].to_vec()
        };

        assert_eq!(field(12), vec![0u8; 24], "the LM response is zeroes: this is v2 only");
        let nt = field(20);
        assert!(nt.len() > 24, "a v2 response is the proof plus the blob");
        assert_eq!(&nt[16..24], &[0x01, 0x01, 0, 0, 0, 0, 0, 0], "the blob's header follows the proof");
        assert!(
            nt.windows(10).any(|window| window == b"\x02\x00\x06\x00DOMAIN"),
            "the server's target info is handed back verbatim"
        );

        assert_eq!(field(28), utf16("DOMAIN"));
        assert_eq!(field(36), utf16("ada"));
        assert_eq!(field(44), utf16("LAPTOP"));
    }

    #[test]
    fn the_response_depends_on_the_password_and_the_nonce() {
        let challenge = parse_challenge(&challenge_bytes()).unwrap();
        let one = authenticate(&challenge, "ada", "hunter2", "D", "W", 1_700_000_000, [1; 8]);
        let wrong_password = authenticate(&challenge, "ada", "hunter3", "D", "W", 1_700_000_000, [1; 8]);
        let other_nonce = authenticate(&challenge, "ada", "hunter2", "D", "W", 1_700_000_000, [2; 8]);

        assert_ne!(one, wrong_password);
        assert_ne!(one, other_nonce);
    }

    #[test]
    fn windows_counts_from_1601() {
        // 1970-01-01 is 11644473600 seconds after 1601-01-01.
        assert_eq!(filetime(0), 116_444_736_000_000_000);
    }
}
