use std::cmp::Ordering;
use std::collections::BTreeMap;

use oxidean_db::PullReviewRow;

pub(crate) fn latest_unambiguous_reviews_by_author(
    reviews: &[PullReviewRow],
) -> Vec<&PullReviewRow> {
    let mut latest: BTreeMap<&str, (&PullReviewRow, bool)> = BTreeMap::new();
    for review in reviews {
        let author_id = review.author_id.as_str();
        match latest.get(author_id) {
            None => {
                latest.insert(author_id, (review, false));
            }
            Some((previous, _)) => match recency(previous, review) {
                Some(Ordering::Greater) => {
                    latest.insert(author_id, (review, false));
                }
                Some(Ordering::Less) => {}
                Some(Ordering::Equal) | None => {
                    latest.insert(author_id, (previous, true));
                }
            },
        }
    }
    latest
        .into_values()
        .filter_map(|(review, ambiguous)| (!ambiguous).then_some(review))
        .collect()
}

fn recency(previous: &PullReviewRow, candidate: &PullReviewRow) -> Option<Ordering> {
    match (previous.submission_order, candidate.submission_order) {
        (Some(previous), Some(candidate)) => match candidate.cmp(&previous) {
            Ordering::Equal => None,
            ordering => Some(ordering),
        },
        (Some(_), None) => Some(Ordering::Less),
        (None, Some(_)) => Some(Ordering::Greater),
        (None, None) => match candidate.submitted_at.cmp(&previous.submitted_at) {
            Ordering::Equal => None,
            ordering => Some(ordering),
        },
    }
}
