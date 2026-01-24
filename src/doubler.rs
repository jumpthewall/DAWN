use anyhow::{anyhow, Result};
use hickory_proto::op::Message;
use hickory_proto::rr::RecordType;

/// DNS header size in bytes
const HEADER_SIZE: usize = 12;

/// Check if the record type should have its question duplicated
fn should_duplicate(rtype: RecordType) -> bool {
    matches!(rtype, RecordType::A | RecordType::AAAA | RecordType::CNAME)
}

/// Duplicates questions in a DNS query using compression pointers.
///
/// For A, AAAA, or CNAME queries, this function:
/// 1. Parses the query to identify question types
/// 2. Doubles QDCOUNT in the header
/// 3. Appends duplicate questions using DNS compression pointers
///
/// Returns the modified packet bytes, or the original if no duplication needed.
pub fn duplicate_questions(original: &[u8]) -> Result<Vec<u8>> {
    if original.len() < HEADER_SIZE {
        return Err(anyhow!("Packet too short for DNS header"));
    }

    // Parse the message to check question types
    let message = Message::from_vec(original).map_err(|e| anyhow!("Failed to parse DNS message: {}", e))?;

    let questions = message.queries();
    if questions.is_empty() {
        return Ok(original.to_vec());
    }

    // Check if any question needs duplication
    let needs_duplication = questions.iter().any(|q| should_duplicate(q.query_type()));
    if !needs_duplication {
        return Ok(original.to_vec());
    }

    // Build the new packet with duplicated questions
    let mut result = Vec::with_capacity(original.len() + questions.len() * 6);

    // Copy the header
    result.extend_from_slice(&original[..HEADER_SIZE]);

    // Update QDCOUNT (bytes 4-5) to double the question count
    let original_qdcount = u16::from_be_bytes([original[4], original[5]]);
    let new_qdcount = original_qdcount.saturating_mul(2);
    result[4] = (new_qdcount >> 8) as u8;
    result[5] = (new_qdcount & 0xFF) as u8;

    // Find where the question section ends by walking through questions
    let mut offset = HEADER_SIZE;
    let mut question_offsets = Vec::with_capacity(questions.len());

    for _ in 0..original_qdcount {
        // Record where this question's name starts
        question_offsets.push(offset);

        // Skip the name (labels until null or pointer)
        offset = skip_name(original, offset)?;

        // Skip QTYPE (2 bytes) and QCLASS (2 bytes)
        offset += 4;
    }

    // Copy original question section
    result.extend_from_slice(&original[HEADER_SIZE..offset]);

    // Add duplicates using compression pointers
    for (i, q) in questions.iter().enumerate() {
        if should_duplicate(q.query_type()) {
            // Compression pointer to original name: 0xC000 | offset
            let name_offset = question_offsets[i] as u16;
            let pointer = 0xC000 | name_offset;
            result.push((pointer >> 8) as u8);
            result.push((pointer & 0xFF) as u8);

            // QTYPE (2 bytes)
            let qtype: u16 = q.query_type().into();
            result.push((qtype >> 8) as u8);
            result.push((qtype & 0xFF) as u8);

            // QCLASS (2 bytes) - IN class = 1
            let qclass: u16 = q.query_class().into();
            result.push((qclass >> 8) as u8);
            result.push((qclass & 0xFF) as u8);
        } else {
            // For non-duplicated types, still need to add a copy
            // Use compression pointer to avoid repeating name
            let name_offset = question_offsets[i] as u16;
            let pointer = 0xC000 | name_offset;
            result.push((pointer >> 8) as u8);
            result.push((pointer & 0xFF) as u8);

            let qtype: u16 = q.query_type().into();
            result.push((qtype >> 8) as u8);
            result.push((qtype & 0xFF) as u8);

            let qclass: u16 = q.query_class().into();
            result.push((qclass >> 8) as u8);
            result.push((qclass & 0xFF) as u8);
        }
    }

    // Copy any remaining data (shouldn't be any for a query, but just in case)
    if offset < original.len() {
        result.extend_from_slice(&original[offset..]);
    }

    Ok(result)
}

/// Skip a DNS name in the wire format, handling labels and compression pointers.
/// Returns the offset after the name.
fn skip_name(data: &[u8], mut offset: usize) -> Result<usize> {
    loop {
        if offset >= data.len() {
            return Err(anyhow!("Unexpected end of data while parsing name"));
        }

        let len = data[offset];

        if len == 0 {
            // Null terminator
            return Ok(offset + 1);
        } else if (len & 0xC0) == 0xC0 {
            // Compression pointer (2 bytes)
            return Ok(offset + 2);
        } else if (len & 0xC0) == 0 {
            // Regular label
            offset += 1 + len as usize;
        } else {
            return Err(anyhow!("Invalid label length byte: {:#x}", len));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skip_name_simple() {
        // "example.com" = 7example3com0
        let data = [
            7, b'e', b'x', b'a', b'm', b'p', b'l', b'e',
            3, b'c', b'o', b'm',
            0
        ];
        assert_eq!(skip_name(&data, 0).unwrap(), 13);
    }

    #[test]
    fn test_duplicate_a_query() {
        // Build a simple A query for "test.com"
        let query = vec![
            0x12, 0x34, // ID
            0x01, 0x00, // Flags (standard query)
            0x00, 0x01, // QDCOUNT = 1
            0x00, 0x00, // ANCOUNT = 0
            0x00, 0x00, // NSCOUNT = 0
            0x00, 0x00, // ARCOUNT = 0
            // Question: test.com A IN
            4, b't', b'e', b's', b't',
            3, b'c', b'o', b'm',
            0,
            0x00, 0x01, // QTYPE = A
            0x00, 0x01, // QCLASS = IN
        ];

        let result = duplicate_questions(&query).unwrap();

        // Check QDCOUNT is now 2
        assert_eq!(result[4], 0x00);
        assert_eq!(result[5], 0x02);

        // Original question should be preserved
        assert_eq!(&result[12..26], &query[12..26]);

        // Duplicate should use compression pointer
        assert_eq!(result[26], 0xC0); // Compression pointer high byte
        assert_eq!(result[27], 0x0C); // Points to offset 12
        assert_eq!(result[28], 0x00); // QTYPE high byte
        assert_eq!(result[29], 0x01); // QTYPE = A
        assert_eq!(result[30], 0x00); // QCLASS high byte
        assert_eq!(result[31], 0x01); // QCLASS = IN
    }
}
