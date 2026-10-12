use anchor_lang::prelude::*;
use crate::errors::OblioError;
use crate::state::Bucket;

/// Loads *every* bucket exactly once. Indices are unique and contiguous, so requiring
/// `remaining[i].index == i` and `len == bucket_count` makes omission/duplication impossible.
pub fn load_all_buckets<'info>(
    remaining: &'info [AccountInfo<'info>],
    expected: u16,
) -> Result<Vec<Account<'info, Bucket>>> {
    require_eq!(remaining.len(), expected as usize, OblioError::InvalidBucketSet);
    let mut out = Vec::with_capacity(remaining.len());
    for (i, info) in remaining.iter().enumerate() {
        let bucket = Account::<Bucket>::try_from(info)?;
        require_eq!(bucket.index as usize, i, OblioError::InvalidBucketSet);
        out.push(bucket);
    }
    Ok(out)
}
