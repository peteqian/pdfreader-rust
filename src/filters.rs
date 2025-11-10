use std::os::raw::c_ulong;

/// Decodes FlateDecode (zlib) compressed stream data.
///
/// This decoder uses the system zlib library via FFI. It automatically grows
/// the buffer if needed to accommodate the decompressed output.
pub fn flate_decode(data: &[u8]) -> Result<Vec<u8>, String> {
    const Z_OK: i32 = 0;
    const Z_BUF_ERROR: i32 = -5;

    let mut capacity = data.len().saturating_mul(4).max(1024);
    loop {
        let mut buffer = vec![0u8; capacity];
        let mut dest_len = capacity as c_ulong;
        let src_len = data.len() as c_ulong;
        let status = unsafe {
            uncompress(
                buffer.as_mut_ptr(),
                &mut dest_len as *mut c_ulong,
                data.as_ptr(),
                src_len,
            )
        };

        match status {
            Z_OK => {
                buffer.truncate(dest_len as usize);
                return Ok(buffer);
            }
            Z_BUF_ERROR => {
                capacity *= 2;
                if capacity > 64 * 1024 * 1024 {
                    return Err("Stream too large to decode".to_string());
                }
            }
            code => {
                return Err(format!("zlib decode error: {}", code));
            }
        }
    }
}

/// Decodes ASCII85Decode (base85) encoded stream data.
///
/// ASCII85 (also called Base85) encodes 4 bytes into 5 ASCII characters.
/// This is commonly used to make binary data safe for text transmission.
pub fn ascii85_decode(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    let mut buffer = 0u32;
    let mut count = 0;
    let text = String::from_utf8_lossy(data);

    for ch in text.chars() {
        match ch {
            // Whitespace is ignored
            ' ' | '\t' | '\n' | '\r' | '\x0c' => continue,
            // Special case: 'z' represents 4 zero bytes
            'z' => {
                if count != 0 {
                    return Err("Invalid ASCII85: 'z' in middle of group".to_string());
                }
                result.extend_from_slice(&[0, 0, 0, 0]);
            }
            // End marker
            '~' => break,
            // Regular base85 character
            c if c >= '!' && c <= 'u' => {
                buffer = buffer * 85 + (c as u32 - 33);
                count += 1;

                if count == 5 {
                    // Convert 5-character group to 4 bytes
                    result.push((buffer >> 24) as u8);
                    result.push((buffer >> 16) as u8);
                    result.push((buffer >> 8) as u8);
                    result.push(buffer as u8);
                    buffer = 0;
                    count = 0;
                }
            }
            _ => return Err(format!("Invalid ASCII85 character: {}", ch)),
        }
    }

    // Handle remaining characters (less than 5)
    if count > 0 {
        // Pad with 'u' characters
        while count < 5 {
            buffer = buffer * 85 + 84; // 'u' = 84
            count += 1;
        }

        // Decode the padded group, but only use count-1 bytes
        result.push((buffer >> 24) as u8);
        if count > 1 {
            result.push((buffer >> 16) as u8);
        }
        if count > 2 {
            result.push((buffer >> 8) as u8);
        }
        if count > 3 {
            result.push(buffer as u8);
        }
    }

    Ok(result)
}

/// Decodes ASCIIHexDecode encoded stream data.
///
/// Converts hex-encoded data (0-9, A-F) back to binary.
/// Example: "48656C6C6F" → "Hello"
pub fn ascii_hex_decode(data: &[u8]) -> Result<Vec<u8>, String> {
    let text = String::from_utf8_lossy(data);
    let mut result = Vec::new();
    let mut chars = text.chars().filter(|c| !c.is_whitespace());

    loop {
        let first = match chars.next() {
            Some(c) => c,
            None => break,
        };

        // If we hit the end marker, we're done
        if first == '>' {
            break;
        }

        let second = chars.next().unwrap_or('0'); // Pad with '0' if odd number of chars

        let first_digit = first.to_digit(16).ok_or_else(|| {
            format!("Invalid hex character in ASCIIHexDecode: {}", first)
        })? as u8;

        let second_digit = second.to_digit(16).ok_or_else(|| {
            format!("Invalid hex character in ASCIIHexDecode: {}", second)
        })? as u8;

        result.push((first_digit << 4) | second_digit);
    }

    Ok(result)
}

/// Decodes LZWDecode compressed stream data.
///
/// LZW (Lempel-Ziv-Welch) is a dictionary-based compression algorithm.
/// Commonly used in older PDFs.
pub fn lzw_decode(data: &[u8]) -> Result<Vec<u8>, String> {
    // Simple LZW decoder
    let mut result = Vec::new();
    let mut dict: Vec<Vec<u8>> = Vec::new();

    // Initialize dictionary with single-byte sequences (0-255)
    for i in 0..256 {
        dict.push(vec![i as u8]);
    }

    let mut code_size = 9; // Start with 9-bit codes
    let clear_table = 256;
    let eod_code = 257;
    let mut dict_size = 258;

    let mut bit_pos = 0;
    let mut prev_code = None;

    while bit_pos < data.len() * 8 {
        // Read a code
        let code = read_bits(data, &mut bit_pos, code_size)?;

        if code == clear_table {
            // Reset dictionary
            dict.truncate(258);
            dict_size = 258;
            code_size = 9;
            prev_code = None;
            continue;
        }

        if code == eod_code {
            break;
        }

        let sequence = if code < dict.len() {
            dict[code].clone()
        } else if code == dict_size && prev_code.is_some() {
            // Special case: code is the next dictionary entry
            let prev_idx: usize = prev_code.unwrap();
            let mut seq = dict[prev_idx].clone();
            seq.push(seq[0]); // Append first byte of previous sequence
            seq
        } else {
            return Err(format!("Invalid LZW code: {}", code));
        };

        result.extend_from_slice(&sequence);

        // Add to dictionary
        if let Some(prev) = prev_code {
            if dict_size < 4096 {
                let mut new_entry = dict[prev].clone();
                new_entry.push(sequence[0]);
                dict.push(new_entry);
                dict_size += 1;

                // Increase code size when dictionary doubles
                if dict_size == (1 << code_size) && code_size < 12 {
                    code_size += 1;
                }
            }
        }

        prev_code = Some(code);
    }

    Ok(result)
}

/// Helper function to read N bits from a byte buffer
fn read_bits(data: &[u8], bit_pos: &mut usize, num_bits: usize) -> Result<usize, String> {
    let mut value = 0;

    for _ in 0..num_bits {
        let byte_pos = *bit_pos / 8;
        let bit_offset = 7 - (*bit_pos % 8);

        if byte_pos >= data.len() {
            return Err("Unexpected end of LZW data".to_string());
        }

        let bit = (data[byte_pos] >> bit_offset) & 1;
        value = (value << 1) | (bit as usize);
        *bit_pos += 1;
    }

    Ok(value)
}

// FFI binding to system zlib library
#[link(name = "z")]
unsafe extern "C" {
    fn uncompress(
        dest: *mut u8,
        dest_len: *mut c_ulong,
        source: *const u8,
        source_len: c_ulong,
    ) -> i32;
}
