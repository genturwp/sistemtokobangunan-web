//! Indonesian mobile numbers, normalised to `+628…`.

/// The input is not an Indonesian mobile number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PhoneError {
    #[error("not an Indonesian mobile number")]
    Invalid,
}

/// Normalises an Indonesian mobile number to `+628…`.
///
/// Removes whitespace, `-`, `.`, `(` and `)`; turns a leading `0` or `62` into `+62`, and puts
/// `+62` before a leading `8` (the number typed without its `0`). The result
/// must be `+628` followed by 8 to 11 ASCII digits (the pattern `^\+628\d{8,11}$`).
pub fn normalize_id_mobile(input: &str) -> Result<String, PhoneError> {
    let compact: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '-' | '.' | '(' | ')'))
        .collect();

    let normalized = if let Some(rest) = compact.strip_prefix('0') {
        format!("+62{rest}")
    } else if compact.starts_with("62") {
        format!("+{compact}")
    } else if compact.starts_with('8') {
        format!("+62{compact}")
    } else {
        compact
    };

    let subscriber = normalized.strip_prefix("+628").ok_or(PhoneError::Invalid)?;
    if (8..=11).contains(&subscriber.len()) && subscriber.bytes().all(|b| b.is_ascii_digit()) {
        Ok(normalized)
    } else {
        Err(PhoneError::Invalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_common_formats() {
        for input in [
            "0812-3456-7890",
            "+62 812 3456 7890",
            "62812.3456.7890",
            "(0812) 3456 7890",
        ] {
            assert_eq!(
                normalize_id_mobile(input),
                Ok("+6281234567890".to_string()),
                "input {input:?}"
            );
        }
    }

    #[test]
    fn rejects_landline_and_short() {
        for input in ["021-5551234", "0812345", "", "abc"] {
            assert_eq!(
                normalize_id_mobile(input),
                Err(PhoneError::Invalid),
                "input {input:?}"
            );
        }
    }

    #[test]
    fn accepts_without_leading_zero() {
        for input in ["812-3456-7890", "812 3456 7890", "(812) 3456 7890"] {
            assert_eq!(
                normalize_id_mobile(input),
                Ok("+6281234567890".to_string()),
                "input {input:?}"
            );
        }
        for input in ["21-5551234", "812345", "8123-4567-8901-2"] {
            assert_eq!(
                normalize_id_mobile(input),
                Err(PhoneError::Invalid),
                "input {input:?}"
            );
        }
    }

    #[test]
    fn accepts_8_to_11_digits_after_8() {
        // Digits after `+628`: 8 and 11 are the bounds.
        assert_eq!(normalize_id_mobile("0812345678"), Ok("+62812345678".into()));
        assert_eq!(
            normalize_id_mobile("0812345678901"),
            Ok("+62812345678901".into())
        );
        assert_eq!(normalize_id_mobile("081234567"), Err(PhoneError::Invalid));
        assert_eq!(
            normalize_id_mobile("08123456789012"),
            Err(PhoneError::Invalid)
        );
    }

    #[test]
    fn rejects_other_characters_and_prefixes() {
        for input in [
            "0812-3456-789a",
            "+0812 3456 7890",
            "62+812 3456 7890",
            "+62 0812 3456 7890",
            "++6281234567890",
            "08１2-3456-7890",
        ] {
            assert_eq!(
                normalize_id_mobile(input),
                Err(PhoneError::Invalid),
                "input {input:?}"
            );
        }
    }
}
