//! Fractional position keys for sibling order, built for the Automerge proof
//! (Loro has its own). A key sorts between its neighbours as a plain string,
//! so moving an item writes one key and never touches the others.
//!
//! Two copies inserting at the same spot at the same time can produce the
//! same key; readers break the tie by node id, so the order is still the
//! same everywhere.

const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

fn digit(c: u8) -> usize {
    DIGITS.iter().position(|&d| d == c).unwrap_or(0)
}

/// A key strictly between `lo` and `hi` (`None` means no bound). If
/// `lo >= hi` (equal keys from a tie), `hi` is ignored and the key goes
/// right after `lo`.
pub fn between(lo: Option<&str>, hi: Option<&str>) -> String {
    let lo = lo.unwrap_or("");
    let hi = hi.filter(|h| lo < *h);
    midpoint(lo.as_bytes(), hi.map(str::as_bytes))
}

fn midpoint(a: &[u8], b: Option<&[u8]>) -> String {
    if let Some(b) = b {
        let mut n = 0;
        while n < b.len() && a.get(n).copied().unwrap_or(b'0') == b[n] {
            n += 1;
        }
        if n > 0 {
            let rest_a = &a[n.min(a.len())..];
            let mut out = String::from_utf8_lossy(&b[..n]).into_owned();
            out.push_str(&midpoint(rest_a, Some(&b[n..])));
            return out;
        }
    }
    let da = a.first().map_or(0, |&c| digit(c));
    let db = b.map_or(DIGITS.len(), |b| digit(b[0]));
    if db - da > 1 {
        return (DIGITS[(da + db).div_ceil(2)] as char).to_string();
    }
    if let Some(b) = b
        && b.len() > 1
    {
        return (b[0] as char).to_string();
    }
    let mut out = (DIGITS[da] as char).to_string();
    out.push_str(&midpoint(a.get(1..).unwrap_or(&[]), None));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workload::Rng;

    #[test]
    fn appending_and_prepending_stay_ordered() {
        let mut keys = vec![between(None, None)];
        for _ in 0..200 {
            let last = keys.last().cloned();
            keys.push(between(last.as_deref(), None));
            let first = keys[0].clone();
            keys.insert(0, between(None, Some(&first)));
        }
        assert!(keys.windows(2).all(|w| w[0] < w[1]), "{keys:?}");
        assert!(keys.iter().all(|k| !k.ends_with('0')));
    }

    #[test]
    fn random_inserts_stay_strictly_ordered() {
        let mut rng = Rng::new(7);
        let mut keys: Vec<String> = vec![between(None, None)];
        for _ in 0..5_000 {
            let i = rng.below(keys.len() + 1);
            let lo = i.checked_sub(1).map(|j| keys[j].as_str());
            let hi = keys.get(i).map(String::as_str);
            let k = between(lo, hi);
            if let Some(lo) = lo {
                assert!(lo < k.as_str(), "{lo} < {k}");
            }
            if let Some(hi) = hi {
                assert!(k.as_str() < hi, "{k} < {hi}");
            }
            keys.insert(i, k);
        }
        let longest = keys.iter().map(String::len).max().unwrap();
        assert!(longest < 64, "keys grew to {longest} chars");
    }

    #[test]
    fn equal_neighbours_from_a_tie_still_give_a_key_after() {
        let k = between(Some("V"), Some("V"));
        assert!(k.as_str() > "V");
    }
}
