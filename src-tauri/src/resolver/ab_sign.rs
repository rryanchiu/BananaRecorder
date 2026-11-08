// Douyin a_bogus signing — 1:1 port of src/ab_sign.py.
//
// NOTE: the Python implementation works in "code point space" (str of
// ord() values 0-255), never re-encoding to UTF-8 mid-pipeline. This port
// mirrors that with Vec<u32> code-point buffers.

const WINDOW_ENV: &str = "1920|1080|1920|1040|0|30|0|0|1872|92|1920|1040|1857|92|1|24|Win32";

// ---------------------------------------------------------------------------
// RC4 over code points (ab_sign.py rc4_encrypt)
// ---------------------------------------------------------------------------

pub fn rc4_encrypt(plaintext: &[u32], key: &[u32]) -> Vec<u32> {
    let mut s: Vec<u32> = (0..256).collect();
    let mut j = 0usize;
    for i in 0..256 {
        j = (j + s[i] as usize + key[i % key.len()] as usize) % 256;
        s.swap(i, j);
    }
    let (mut i, mut j) = (0usize, 0usize);
    let mut out = Vec::with_capacity(plaintext.len());
    for &ch in plaintext {
        i = (i + 1) % 256;
        j = (j + s[i] as usize) % 256;
        s.swap(i, j);
        let t = ((s[i] + s[j]) % 256) as usize;
        out.push(s[t] ^ ch);
    }
    out
}

// ---------------------------------------------------------------------------
// SM3 (ab_sign.py SM3 class) — reg-init and layout kept identical
// ---------------------------------------------------------------------------

const SM3_INIT: [u32; 8] = [
    1937774191, 1226093241, 388252375, 3666478592, 2842636476, 372324522, 3817729613, 2969243214,
];

fn left_rotate(x: u32, n: u32) -> u32 {
    x.rotate_left(n % 32)
}

fn t_j(j: usize) -> u32 {
    if j < 16 {
        2043430169 // 0x79CC4519
    } else {
        2055708042 // 0x7A879D8A
    }
}

fn ff(j: usize, x: u32, y: u32, z: u32) -> u32 {
    if j < 16 {
        x ^ y ^ z
    } else {
        (x & y) | (x & z) | (y & z)
    }
}

fn gg(j: usize, x: u32, y: u32, z: u32) -> u32 {
    if j < 16 {
        x ^ y ^ z
    } else {
        (!x & z) | (x & y) // (!x) & z | (x & y)? careful: python is (~x & z) | (x & y)
    }
}

struct Sm3 {
    reg: [u32; 8],
    chunk: Vec<u8>,
    size: usize,
}

impl Sm3 {
    fn new() -> Self {
        Self {
            reg: SM3_INIT,
            chunk: Vec::new(),
            size: 0,
        }
    }

    /// Mirrors the python write() loop exactly (including its chunk slicing).
    fn write(&mut self, a: &[u8]) {
        self.size += a.len();
        let mut f = 64 - self.chunk.len();
        if a.len() < f {
            self.chunk.extend_from_slice(a);
        } else {
            self.chunk.extend_from_slice(&a[..f]);
            while self.chunk.len() >= 64 {
                let block: Vec<u8> = self.chunk[..64].to_vec();
                self.compress(&block);
                if f < a.len() {
                    self.chunk = a[f..(f + 64).min(a.len())].to_vec();
                } else {
                    self.chunk.clear();
                }
                f += 64;
            }
        }
    }

    fn fill(&mut self) {
        let bit_length = (self.size as u64) * 8;
        self.chunk.push(0x80);
        let mut padding_pos = (self.chunk.len()) % 64;
        if 64 - padding_pos < 8 {
            padding_pos -= 64;
        }
        while padding_pos < 56 {
            self.chunk.push(0);
            padding_pos += 1;
        }
        // message length: high 32 bits then low 32 bits, big endian
        let high = (bit_length / 4_294_967_296) as u32;
        let low = (bit_length & 0xFFFF_FFFF) as u32;
        for i in 0..4 {
            self.chunk.push((high >> (8 * (3 - i))) as u8 & 0xFF);
        }
        for i in 0..4 {
            self.chunk.push((low >> (8 * (3 - i))) as u8 & 0xFF);
        }
    }

    fn compress(&mut self, data: &[u8]) {
        let mut w = vec![0u32; 132];
        for t in 0..16 {
            w[t] = ((data[4 * t] as u32) << 24)
                | ((data[4 * t + 1] as u32) << 16)
                | ((data[4 * t + 2] as u32) << 8)
                | (data[4 * t + 3] as u32);
        }
        for j in 16..68 {
            let a = w[j - 16] ^ w[j - 9] ^ left_rotate(w[j - 3], 15);
            w[j] = (a ^ left_rotate(a, 15) ^ left_rotate(a, 23))
                ^ left_rotate(w[j - 13], 7)
                ^ w[j - 6];
        }
        for j in 0..64 {
            w[j + 68] = w[j] ^ w[j + 4];
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.reg;
        for j in 0..64 {
            let ss1 = left_rotate(
                left_rotate(a, 12)
                    .wrapping_add(e)
                    .wrapping_add(left_rotate(t_j(j), j as u32)),
                7,
            );
            let ss2 = ss1 ^ left_rotate(a, 12);
            let tt1 = ff(j, a, b, c)
                .wrapping_add(d)
                .wrapping_add(ss2)
                .wrapping_add(w[j + 68]);
            let tt2 = gg(j, e, f, g)
                .wrapping_add(h)
                .wrapping_add(ss1)
                .wrapping_add(w[j]);
            d = c;
            c = left_rotate(b, 9);
            b = a;
            a = tt1;
            h = g;
            g = left_rotate(f, 19);
            f = e;
            e = tt2 ^ left_rotate(tt2, 9) ^ left_rotate(tt2, 17);
        }

        self.reg[0] ^= a;
        self.reg[1] ^= b;
        self.reg[2] ^= c;
        self.reg[3] ^= d;
        self.reg[4] ^= e;
        self.reg[5] ^= f;
        self.reg[6] ^= g;
        self.reg[7] ^= h;
    }

    fn finish(&mut self) -> Vec<u8> {
        self.fill();
        for block in 0..(self.chunk.len() / 64) {
            let slice: Vec<u8> = self.chunk[block * 64..(block + 1) * 64].to_vec();
            self.compress(&slice);
        }
        let mut result = Vec::with_capacity(32);
        for word in &self.reg {
            result.extend_from_slice(&word.to_be_bytes());
        }
        self.reset();
        result
    }

    fn reset(&mut self) {
        self.reg = SM3_INIT;
        self.chunk.clear();
        self.size = 0;
    }
}

/// One-shot sm3 digest (ab_sign.py SM3().sum(...)).
pub fn sm3_sum(data: &[u8]) -> Vec<u8> {
    let mut sm3 = Sm3::new();
    sm3.write(data);
    sm3.finish()
}

// ---------------------------------------------------------------------------
// Custom base64 over code points (ab_sign.py result_encrypt)
// ---------------------------------------------------------------------------

fn table(num: &str) -> &'static str {
    match num {
        "s0" => "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=",
        "s1" => "Dkdpgh4ZKsQB80/Mfvw36XI1R25+WUAlEi7NLboqYTOPuzmFjJnryx9HVGcaStCe=",
        "s2" => "Dkdpgh4ZKsQB80/Mfvw36XI1R25-WUAlEi7NLboqYTOPuzmFjJnryx9HVGcaStCe=",
        "s3" => "ckdp1h4ZKsUB80/Mfvw36XIgR25+WQAlEi7NLboqYTOPuzmFjJnryx9HVGDaStCe",
        _ => "Dkdpgh2ZmsQB80/MfvV36XI1R45-WUAlEixNLwoqYTOPuzKFjJnry79HbGcaStCe", // s4
    }
}

fn get_long_int(round: usize, long: &[u32]) -> u32 {
    let base = round * 3;
    let c1 = long.get(base).copied().unwrap_or(0);
    let c2 = long.get(base + 1).copied().unwrap_or(0);
    let c3 = long.get(base + 2).copied().unwrap_or(0);
    (c1 << 16) | (c2 << 8) | c3
}

pub fn result_encrypt(long: &[u32], num: &str) -> String {
    let encoding_table = table(num).as_bytes();
    let masks: [u32; 4] = [16515072, 258048, 4032, 63];
    let shifts: [u32; 4] = [18, 12, 6, 0];

    let mut result = String::new();
    let mut round = 0usize;
    let mut long_int = get_long_int(round, long);
    // python: math.ceil(len / 3 * 4) — divide-then-ceil, NOT ceil-then-multiply
    let total_chars = (long.len() * 4).div_ceil(3);

    for i in 0..total_chars {
        if i / 4 != round {
            round += 1;
            long_int = get_long_int(round, long);
        }
        let index = i % 4;
        let char_index = ((long_int & masks[index]) >> shifts[index]) as usize;
        result.push(encoding_table[char_index] as char);
    }
    result
}

// ---------------------------------------------------------------------------
// random prefix + bb assembly (ab_sign.py generate_* / generate_rc4_bb_str)
// ---------------------------------------------------------------------------

fn gener_random(random_num: u32, option: [u8; 2]) -> Vec<u32> {
    let byte1 = (random_num & 255) as u8;
    let byte2 = ((random_num >> 8) & 255) as u8;
    vec![
        ((byte1 & 170) | (option[0] & 85)) as u32,
        ((byte1 & 85) | (option[0] & 170)) as u32,
        ((byte2 & 170) | (option[1] & 85)) as u32,
        ((byte2 & 85) | (option[1] & 170)) as u32,
    ]
}

/// Fixed "random" prefix — same values as the python/js version.
fn generate_random_str() -> Vec<u32> {
    let mut out = Vec::new();
    out.extend(gener_random(1234, [3, 45]));
    out.extend(gener_random(9876, [1, 0]));
    out.extend(gener_random(5555, [1, 5]));
    out
}

fn split_to_bytes(num: u64) -> [u8; 4] {
    [
        ((num >> 24) & 255) as u8,
        ((num >> 16) & 255) as u8,
        ((num >> 8) & 255) as u8,
        (num & 255) as u8,
    ]
}

fn generate_rc4_bb_str(url_search_params: &str, user_agent: &str, now_ms: u64) -> Vec<u32> {
    const SUFFIX: &str = "cus";
    let arguments: [u64; 3] = [0, 1, 14];

    let start_time = now_ms;
    let end_time = start_time + 100;

    // 1. double-sm3 of (url params + suffix) — python concatenates the
    // strings first, THEN hashes twice
    let mut combined = url_search_params.to_string();
    combined.push_str(SUFFIX);
    let url_params_list = sm3_sum(&sm3_sum(combined.as_bytes()));

    // 2. double-sm3 of the suffix
    let cus = sm3_sum(&sm3_sum(SUFFIX.as_bytes()));

    // 3. ua: rc4 with key [0,1,14] → custom base64 (s3) → sm3
    let ua_key: [u32; 3] = [0, 1, 14];
    let ua_points: Vec<u32> = user_agent.chars().map(|c| c as u32).collect();
    let ua_rc4 = rc4_encrypt(&ua_points, &ua_key);
    let ua_encoded = result_encrypt(&ua_rc4, "s3");
    let ua = sm3_sum(ua_encoded.as_bytes());

    let mut b = vec![0u32; 73];
    b[8] = 3;
    b[10] = end_time as u32;
    let aid: u32 = 6383;
    let page_id: u32 = 110624;
    b[16] = start_time as u32;
    b[18] = 44;

    // start time bytes
    let st = split_to_bytes(start_time);
    b[20] = st[0] as u32;
    b[21] = st[1] as u32;
    b[22] = st[2] as u32;
    b[23] = st[3] as u32;
    b[24] = ((start_time >> 32) & 255) as u32;
    b[25] = ((start_time >> 40) & 255) as u32;

    // arguments
    let arg0 = split_to_bytes(arguments[0]);
    b[26] = arg0[0] as u32;
    b[27] = arg0[1] as u32;
    b[28] = arg0[2] as u32;
    b[29] = arg0[3] as u32;
    b[30] = ((arguments[1] >> 8) & 255) as u32;
    b[31] = (arguments[1] & 255) as u32;
    let arg1 = split_to_bytes(arguments[1]);
    b[32] = arg1[0] as u32;
    b[33] = arg1[1] as u32;
    let arg2 = split_to_bytes(arguments[2]);
    b[34] = arg2[0] as u32;
    b[35] = arg2[1] as u32;
    b[36] = arg2[2] as u32;
    b[37] = arg2[3] as u32;

    // encrypted results
    b[38] = url_params_list[21] as u32;
    b[39] = url_params_list[22] as u32;
    b[40] = cus[21] as u32;
    b[41] = cus[22] as u32;
    b[42] = ua[23] as u32;
    b[43] = ua[24] as u32;

    // end time bytes
    let et = split_to_bytes(end_time);
    b[44] = et[0] as u32;
    b[45] = et[1] as u32;
    b[46] = et[2] as u32;
    b[47] = et[3] as u32;
    b[48] = b[8];
    b[49] = ((end_time >> 32) & 255) as u32;
    b[50] = ((end_time >> 40) & 255) as u32;

    // config entries
    b[51] = page_id;
    let pid = split_to_bytes(page_id as u64);
    b[52] = pid[0] as u32;
    b[53] = pid[1] as u32;
    b[54] = pid[2] as u32;
    b[55] = pid[3] as u32;
    b[56] = aid;
    b[57] = aid & 255;
    b[58] = (aid >> 8) & 255;
    b[59] = (aid >> 16) & 255;
    b[60] = (aid >> 24) & 255;

    // window env
    let window_env_list: Vec<u32> = WINDOW_ENV.chars().map(|c| c as u32).collect();
    b[64] = window_env_list.len() as u32;
    b[65] = b[64] & 255;
    b[66] = (b[64] >> 8) & 255;
    b[69] = 0;
    b[70] = 0;
    b[71] = 0;

    // checksum
    b[72] = b[18]
        ^ b[20]
        ^ b[26]
        ^ b[30]
        ^ b[38]
        ^ b[40]
        ^ b[42]
        ^ b[21]
        ^ b[27]
        ^ b[31]
        ^ b[35]
        ^ b[39]
        ^ b[41]
        ^ b[43]
        ^ b[22]
        ^ b[28]
        ^ b[32]
        ^ b[36]
        ^ b[23]
        ^ b[29]
        ^ b[33]
        ^ b[37]
        ^ b[44]
        ^ b[45]
        ^ b[46]
        ^ b[47]
        ^ b[48]
        ^ b[49]
        ^ b[50]
        ^ b[24]
        ^ b[25]
        ^ b[52]
        ^ b[53]
        ^ b[54]
        ^ b[55]
        ^ b[57]
        ^ b[58]
        ^ b[59]
        ^ b[60]
        ^ b[65]
        ^ b[66]
        ^ b[70]
        ^ b[71];

    let mut bb: Vec<u32> = vec![
        b[18], b[20], b[52], b[26], b[30], b[34], b[58], b[38], b[40], b[53], b[42], b[21], b[27],
        b[54], b[55], b[31], b[35], b[57], b[39], b[41], b[43], b[22], b[28], b[32], b[60], b[36],
        b[23], b[29], b[33], b[37], b[44], b[45], b[59], b[46], b[47], b[48], b[49], b[50], b[24],
        b[25], b[65], b[66], b[70], b[71],
    ];
    bb.extend(window_env_list);
    bb.push(b[72]);

    rc4_encrypt(&bb, &[121])
}

/// Debug helper exposing the raw rc4(bb) code points for differential tests.
pub fn debug_rc4_bb_codes(url_search_params: &str, user_agent: &str, now_ms: u64) -> Vec<u32> {
    generate_rc4_bb_str(url_search_params, user_agent, now_ms)
}

/// ab_sign with an injectable timestamp (used for test vectors).
pub fn ab_sign_with_now(url_search_params: &str, user_agent: &str, now_ms: u64) -> String {
    let mut combined = generate_random_str();
    combined.extend(generate_rc4_bb_str(url_search_params, user_agent, now_ms));
    result_encrypt(&combined, "s4") + "="
}

/// ab_sign with the current time — the public entry point.
pub fn ab_sign(url_search_params: &str, user_agent: &str) -> String {
    let now_ms = chrono::Utc::now().timestamp_millis() as u64;
    ab_sign_with_now(url_search_params, user_agent, now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sm3_matches_known_vector() {
        // standard SM3 test vector: SM3("abc")
        let digest = sm3_sum(b"abc");
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "66c7f0f462eeedd9d1f2d46bdc10e4e24167c4875cf2f7a2297da02b8f4ba8e0"
        );
    }

    #[test]
    fn result_encrypt_uses_s4_table() {
        // deterministic encoding, table-character only output
        let out = result_encrypt(&[1, 2, 3, 4, 5, 6], "s4");
        assert_eq!(out.len(), 8);
        for ch in out.chars() {
            assert!(table("s4").contains(ch), "unexpected char {ch}");
        }
    }

    #[test]
    fn ab_sign_shape_and_determinism() {
        let query = "aid=6383&app_name=douyin_web&web_rid=123456&msToken=";
        let ua = "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 Chrome/116.0.0.0";
        let a = ab_sign_with_now(query, ua, 1_700_000_000_000);
        let b = ab_sign_with_now(query, ua, 1_700_000_000_000);
        assert_eq!(a, b, "same inputs must produce identical signatures");
        assert!(a.ends_with('='), "a_bogus ends with '='");
        assert!(a.len() > 100, "signature should be long, got {}", a.len());
        // timestamp change must change the signature
        let c = ab_sign_with_now(query, ua, 1_700_000_000_100);
        assert_ne!(a, c);
    }
}
