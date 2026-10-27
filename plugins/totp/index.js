// ==============================================================================
// OneLauncher 官方插件: TOTP 动态验证码生成器 (RFC 6238 / RFC 4226)
// ==============================================================================

export const metadata = {
  name: "动态验证码",
  prefix: "otp:",
  default_config: {
    interval: 30,
    algorithm: "SHA1",
  },
};

let totpIcon = "shield";

export function on_setup() {
  totpIcon = launcher.cache_icon("shield");
  console.info("TOTP 动态验证码插件已就绪");
}

export function on_query(input) {
  if (!input || input.trim() === "") {
    return [];
  }

  const { secret, interval, algorithm } = parseParams(input);

  if (!secret) {
    return [{
      id: "totp_empty_secret",
      title: "请输入 Base32 格式 Secret 秘钥",
      subtitle: "示例: otp:JBSWY3DPEHPK3PXP 或 otp:s=xxx&a=sha256&i=30",
      icon: "info",
      badge: "TOTP",
      payload: null,
    }];
  }

  const key = base32Decode(secret);
  if (!key || key.length === 0) {
    return [{
      id: "totp_invalid_secret",
      title: "无效的 Base32 秘钥",
      subtitle: "Base32 字符集仅包含 A-Z 与 2-7，请检查输入",
      icon: "warn",
      badge: "TOTP",
      payload: null,
    }];
  }

  const unixTime = Math.floor(Date.now() / 1000);
  const remaining = interval - (unixTime % interval);
  const code = generateTOTP(key, unixTime, interval, algorithm);

  return [
    {
      id: "totp_result",
      title: code,
      subtitle: `剩余有效时间 ${remaining}s | 算法: ${algorithm} | 周期: ${interval}s (回车复制验证码)`,
      icon: totpIcon,
      badge: "TOTP",
      payload: {
        code: code,
        secret: secret,
        algorithm: algorithm,
        interval: interval,
      },
    },
  ];
}

export function on_select(item) {
  if (!item || !item.payload || !item.payload.code) {
    return;
  }

  launcher.clipboard.write_text(item.payload.code);
  console.info(`已复制 TOTP 动态验证码 [${item.payload.code}] 到剪贴板`);
}

/**
 * 参数解析：支持 otp:XXXX 直接传递 secret，
 * 以及 otp:s[ecret]=xxx&a[lgorithm]=yyy&i[nterval]=zzz 多元化参数，参数名忽略大小写。
 */
function parseParams(rawInput) {
  const cfg = launcher.config || {};
  let defaultInterval = (cfg.interval && Number(cfg.interval)) || 30;
  let defaultAlgorithm = (cfg.algorithm && String(cfg.algorithm).toUpperCase()) || "SHA1";

  let secret = "";
  let interval = defaultInterval;
  let algorithm = defaultAlgorithm;

  const trimmed = rawInput.trim();

  // 检查是否包含类似 s= / secret= / a= / algorithm= / i= / interval= 的键值模式
  const hasKeyPattern = /(?:^|[&])\s*(?:s|secret|a|algorithm|i|interval)\s*=/i.test(trimmed);

  if (hasKeyPattern) {
    const parts = trimmed.split("&");
    for (const part of parts) {
      const eqIdx = part.indexOf("=");
      if (eqIdx !== -1) {
        const key = part.substring(0, eqIdx).trim().toLowerCase();
        const val = part.substring(eqIdx + 1).trim();

        if (key === "s" || key === "secret") {
          secret = val;
        } else if (key === "a" || key === "algorithm") {
          const upper = val.toUpperCase();
          if (upper === "SHA1" || upper === "SHA256" || upper === "SHA512") {
            algorithm = upper;
          }
        } else if (key === "i" || key === "interval") {
          const parsed = parseInt(val, 10);
          if (!isNaN(parsed) && parsed > 0) {
            interval = parsed;
          }
        }
      }
    }
  } else {
    // 默认直接传递整个内容作为 secret
    secret = trimmed;
  }

  return { secret, interval, algorithm };
}

// ==============================================================================
// RFC 3548 / RFC 4648 Base32 解码
// ==============================================================================
function base32Decode(str) {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
  const cleaned = str.toUpperCase().replace(/[\s=]/g, "");
  if (!cleaned) return null;

  let buffer = 0;
  let bitsInBuffer = 0;
  const result = [];

  for (let i = 0; i < cleaned.length; i++) {
    const ch = cleaned[i];
    const index = alphabet.indexOf(ch);
    if (index === -1) {
      return null;
    }

    buffer = (buffer << 5) | index;
    bitsInBuffer += 5;

    if (bitsInBuffer >= 8) {
      bitsInBuffer -= 8;
      const byte = (buffer >> bitsInBuffer) & 0xff;
      result.push(byte);
      if (bitsInBuffer > 0) {
        buffer = buffer & ((1 << bitsInBuffer) - 1);
      } else {
        buffer = 0;
      }
    }
  }

  return new Uint8Array(result);
}

// ==============================================================================
// TOTP 算法实现 (RFC 6238 / RFC 4226)
// ==============================================================================
function generateTOTP(keyBytes, unixTime, interval, algorithm) {
  const counter = BigInt(Math.floor(unixTime / interval));
  const counterBytes = new Uint8Array(8);
  const view = new DataView(counterBytes.buffer);
  view.setBigUint64(0, counter, false);

  let hash;
  if (algorithm === "SHA256") {
    hash = hmac(sha256, 64, keyBytes, counterBytes);
  } else if (algorithm === "SHA512") {
    hash = hmac(sha512, 128, keyBytes, counterBytes);
  } else {
    hash = hmac(sha1, 64, keyBytes, counterBytes);
  }

  const offset = hash[hash.length - 1] & 0x0f;
  const binaryCode =
    ((hash[offset] & 0x7f) << 24) |
    ((hash[offset + 1] & 0xff) << 16) |
    ((hash[offset + 2] & 0xff) << 8) |
    (hash[offset + 3] & 0xff);

  const otp = (binaryCode >>> 0) % 1000000;
  return otp.toString().padStart(6, "0");
}

// ==============================================================================
// 原生 HMAC 实现
// ==============================================================================
function hmac(hashFn, blockSize, key, message) {
  let k = key;
  if (k.length > blockSize) {
    k = hashFn(k);
  }
  if (k.length < blockSize) {
    const padded = new Uint8Array(blockSize);
    padded.set(k);
    k = padded;
  }

  const oKeyPad = new Uint8Array(blockSize);
  const iKeyPad = new Uint8Array(blockSize);
  for (let i = 0; i < blockSize; i++) {
    oKeyPad[i] = k[i] ^ 0x5c;
    iKeyPad[i] = k[i] ^ 0x36;
  }

  const inner = new Uint8Array(blockSize + message.length);
  inner.set(iKeyPad, 0);
  inner.set(message, blockSize);
  const innerHash = hashFn(inner);

  const outer = new Uint8Array(blockSize + innerHash.length);
  outer.set(oKeyPad, 0);
  outer.set(innerHash, blockSize);
  return hashFn(outer);
}

// ==============================================================================
// 原生 SHA-1
// ==============================================================================
function sha1(bytes) {
  const K = [0x5a827999, 0x6ed9eba1, 0x8f1bbcdc, 0xca62c1d6];
  let h0 = 0x67452301, h1 = 0xefcdab89, h2 = 0x98badcfe, h3 = 0x10325476, h4 = 0xc3d2e1f0;

  const len = bytes.length;
  const bitLen = len * 8;
  const padLen = (len % 64 < 56) ? (56 - (len % 64)) : (120 - (len % 64));
  const totalLen = len + padLen + 8;
  const padded = new Uint8Array(totalLen);
  padded.set(bytes);
  padded[len] = 0x80;

  const view = new DataView(padded.buffer);
  view.setUint32(totalLen - 4, bitLen, false);

  const W = new Uint32Array(80);
  for (let i = 0; i < totalLen; i += 64) {
    for (let t = 0; t < 16; t++) {
      W[t] = view.getUint32(i + t * 4, false);
    }
    for (let t = 16; t < 80; t++) {
      const v = W[t - 3] ^ W[t - 8] ^ W[t - 14] ^ W[t - 16];
      W[t] = (v << 1) | (v >>> 31);
    }

    let a = h0, b = h1, c = h2, d = h3, e = h4;
    for (let t = 0; t < 80; t++) {
      let f, k;
      if (t < 20) {
        f = (b & c) | ((~b) & d);
        k = K[0];
      } else if (t < 40) {
        f = b ^ c ^ d;
        k = K[1];
      } else if (t < 60) {
        f = (b & c) | (b & d) | (c & d);
        k = K[2];
      } else {
        f = b ^ c ^ d;
        k = K[3];
      }
      const temp = (((a << 5) | (a >>> 27)) + f + e + k + W[t]) | 0;
      e = d;
      d = c;
      c = (b << 30) | (b >>> 2);
      b = a;
      a = temp;
    }

    h0 = (h0 + a) | 0;
    h1 = (h1 + b) | 0;
    h2 = (h2 + c) | 0;
    h3 = (h3 + d) | 0;
    h4 = (h4 + e) | 0;
  }

  const out = new Uint8Array(20);
  const outView = new DataView(out.buffer);
  outView.setUint32(0, h0, false);
  outView.setUint32(4, h1, false);
  outView.setUint32(8, h2, false);
  outView.setUint32(12, h3, false);
  outView.setUint32(16, h4, false);
  return out;
}

// ==============================================================================
// 原生 SHA-256
// ==============================================================================
function sha256(bytes) {
  const K = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2
  ];

  let h0 = 0x6a09e667, h1 = 0xbb67ae85, h2 = 0x3c6ef372, h3 = 0xa54ff53a;
  let h4 = 0x510e527f, h5 = 0x9b05688c, h6 = 0x1f83d9ab, h7 = 0x5be0cd19;

  const len = bytes.length;
  const bitLen = len * 8;
  const padLen = (len % 64 < 56) ? (56 - (len % 64)) : (120 - (len % 64));
  const totalLen = len + padLen + 8;
  const padded = new Uint8Array(totalLen);
  padded.set(bytes);
  padded[len] = 0x80;

  const view = new DataView(padded.buffer);
  view.setUint32(totalLen - 4, bitLen, false);

  const W = new Uint32Array(64);
  const rotr = (n, x) => (x >>> n) | (x << (32 - n));

  for (let i = 0; i < totalLen; i += 64) {
    for (let t = 0; t < 16; t++) {
      W[t] = view.getUint32(i + t * 4, false);
    }
    for (let t = 16; t < 64; t++) {
      const s0 = rotr(7, W[t - 15]) ^ rotr(18, W[t - 15]) ^ (W[t - 15] >>> 3);
      const s1 = rotr(17, W[t - 2]) ^ rotr(19, W[t - 2]) ^ (W[t - 2] >>> 10);
      W[t] = (W[t - 16] + s0 + W[t - 7] + s1) | 0;
    }

    let a = h0, b = h1, c = h2, d = h3, e = h4, f = h5, g = h6, h = h7;

    for (let t = 0; t < 64; t++) {
      const S1 = rotr(6, e) ^ rotr(11, e) ^ rotr(25, e);
      const ch = (e & f) ^ ((~e) & g);
      const temp1 = (h + S1 + ch + K[t] + W[t]) | 0;
      const S0 = rotr(2, a) ^ rotr(13, a) ^ rotr(22, a);
      const maj = (a & b) ^ (a & c) ^ (b & c);
      const temp2 = (S0 + maj) | 0;

      h = g;
      g = f;
      f = e;
      e = (d + temp1) | 0;
      d = c;
      c = b;
      b = a;
      a = (temp1 + temp2) | 0;
    }

    h0 = (h0 + a) | 0;
    h1 = (h1 + b) | 0;
    h2 = (h2 + c) | 0;
    h3 = (h3 + d) | 0;
    h4 = (h4 + e) | 0;
    h5 = (h5 + f) | 0;
    h6 = (h6 + g) | 0;
    h7 = (h7 + h) | 0;
  }

  const out = new Uint8Array(32);
  const outView = new DataView(out.buffer);
  outView.setUint32(0, h0, false);
  outView.setUint32(4, h1, false);
  outView.setUint32(8, h2, false);
  outView.setUint32(12, h3, false);
  outView.setUint32(16, h4, false);
  outView.setUint32(20, h5, false);
  outView.setUint32(24, h6, false);
  outView.setUint32(28, h7, false);
  return out;
}

// ==============================================================================
// 原生 SHA-512
// ==============================================================================
function sha512(bytes) {
  const K = [
    0x428a2f98d728ae22n, 0x7137449123ef65cdn, 0xb5c0fbcfec4d3b2fn, 0xe9b5dba58189dbbcn,
    0x3956c25bf348b538n, 0x59f111f1b605d019n, 0x923f82a4af194f9bn, 0xab1c5ed5da6d8118n,
    0xd807aa98a3030242n, 0x12835b0145706fben, 0x243185be4ee4b28cn, 0x550c7dc3d5ffb4e2n,
    0x72be5d74f27b896fn, 0x80deb1fe3b1696b1n, 0x9bdc06a725c71235n, 0xc19bf174cf692694n,
    0xe49b69c19ef14ad2n, 0xefbe4786384f25e3n, 0x0fc19dc68b8cd5b5n, 0x240ca1cc77ac9c65n,
    0x2de92c6f592b0275n, 0x4a7484aa6ea6e483n, 0x5cb0a9dcbd41fbd4n, 0x76f988da831153b5n,
    0x983e5152ee66dfabn, 0xa831c66d2db43210n, 0xb00327c898fb213fn, 0xbf597fc7beef0ee4n,
    0xc6e00bf33da88fc2n, 0xd5a79147930aa725n, 0x06ca6351e003826fn, 0x142929670a0e6e70n,
    0x27b70a8546d22ffcn, 0x2e1b21385c26c926n, 0x4d2c6dfc5ac42aedn, 0x53380d139d95b3dfn,
    0x650a73548baf63den, 0x766a0abb3c77b2a8n, 0x81c2c92e47edaee6n, 0x92722c851482353bn,
    0xa2bfe8a14cf10364n, 0xa81a664bbc423001n, 0xc24b8b70d0f89791n, 0xc76c51a30654be30n,
    0xd192e819d6ef5218n, 0xd69906245565a910n, 0xf40e35855771202an, 0x106aa07032bbd1b8n,
    0x19a4c116b8d2d0c8n, 0x1e376c085141ab53n, 0x2748774cdf8eeb99n, 0x34b0bcb5e19b48a8n,
    0x391c0cb3c5c95a63n, 0x4ed8aa4ae3418acbn, 0x5b9cca4f7763e373n, 0x682e6ff3d6b2b8a3n,
    0x748f82ee5defb2fcn, 0x78a5636f43172f60n, 0x84c87814a1f0ab72n, 0x8cc702081a6439ecn,
    0x90befffa23631e28n, 0xa4506cebde82bde9n, 0xbef9a3f7b2c67915n, 0xc67178f2e372532bn,
    0xca273eceea26619cn, 0xd186b8c721c0c207n, 0xeada7dd6cde0eb1en, 0xf57d4f7fee6ed178n,
    0x06f067aa72176fban, 0x0a637dc5a2c898a6n, 0x113f9804bef90daen, 0x1b710b35131c471bn,
    0x28db77f523047d84n, 0x32caab7b40c72493n, 0x3c9ebe0a15c9bebcn, 0x431d67c49c100d4cn,
    0x4cc5d4becb3e42b6n, 0x597f299cfc657e2an, 0x5fcb6fab3ad6faecn, 0x6c44198c4a475817n
  ];

  let H = [
    0x6a09e667f3bcc908n, 0xbb67ae8584caa73bn, 0x3c6ef372fe94f82bn, 0xa54ff53a5f1d36f1n,
    0x510e527fade682d1n, 0x9b05688c2b3e6c1fn, 0x1f83d9abfb41bd6bn, 0x5be0cd19137e2179n
  ];

  const len = bytes.length;
  const bitLen = BigInt(len) * 8n;
  const padLen = (len % 128 < 112) ? (112 - (len % 128)) : (240 - (len % 128));
  const totalLen = len + padLen + 16;
  const padded = new Uint8Array(totalLen);
  padded.set(bytes);
  padded[len] = 0x80;

  const view = new DataView(padded.buffer);
  view.setBigUint64(totalLen - 8, bitLen, false);

  const mask64 = 0xffffffffffffffffn;
  const rotr64 = (n, x) => ((x >> BigInt(n)) | (x << BigInt(64 - n))) & mask64;

  const W = new BigUint64Array(80);

  for (let i = 0; i < totalLen; i += 128) {
    for (let t = 0; t < 16; t++) {
      W[t] = view.getBigUint64(i + t * 8, false);
    }
    for (let t = 16; t < 80; t++) {
      const s0 = rotr64(1, W[t - 15]) ^ rotr64(8, W[t - 15]) ^ (W[t - 15] >> 7n);
      const s1 = rotr64(19, W[t - 2]) ^ rotr64(61, W[t - 2]) ^ (W[t - 2] >> 6n);
      W[t] = (W[t - 16] + s0 + W[t - 7] + s1) & mask64;
    }

    let [a, b, c, d, e, f, g, h] = H;

    for (let t = 0; t < 80; t++) {
      const S1 = rotr64(14, e) ^ rotr64(18, e) ^ rotr64(41, e);
      const ch = (e & f) ^ ((~e & mask64) & g);
      const temp1 = (h + S1 + ch + K[t] + W[t]) & mask64;
      const S0 = rotr64(28, a) ^ rotr64(34, a) ^ rotr64(39, a);
      const maj = (a & b) ^ (a & c) ^ (b & c);
      const temp2 = (S0 + maj) & mask64;

      h = g;
      g = f;
      f = e;
      e = (d + temp1) & mask64;
      d = c;
      c = b;
      b = a;
      a = (temp1 + temp2) & mask64;
    }

    H[0] = (H[0] + a) & mask64;
    H[1] = (H[1] + b) & mask64;
    H[2] = (H[2] + c) & mask64;
    H[3] = (H[3] + d) & mask64;
    H[4] = (H[4] + e) & mask64;
    H[5] = (H[5] + f) & mask64;
    H[6] = (H[6] + g) & mask64;
    H[7] = (H[7] + h) & mask64;
  }

  const out = new Uint8Array(64);
  const outView = new DataView(out.buffer);
  for (let i = 0; i < 8; i++) {
    outView.setBigUint64(i * 8, H[i], false);
  }
  return out;
}
