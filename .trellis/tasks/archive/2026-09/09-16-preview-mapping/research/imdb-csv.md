# IMDb CSV evidence — reviewed 2026-09-17

Official ratings help confirms CSV export from Your Ratings:
https://help.imdb.com/article/imdb/track-movies-tv/faq-for-imdb-ratings/G67Y87TFYYP6TWAV

It does not publish a versioned CSV schema. The adapter therefore validates
the conventional export headers Const, Your Rating and Title Type, with optional
Title. These header/type spellings are an explicit supported input contract,
not a claim of an immutable upstream API. Synthetic fixtures are parser evidence,
not a live account export. A future format change must fail clearly for review.

The approved child requires movie/tvMovie/short -> movie and tvSeries/tvMiniSeries
-> tv. Other types, including episodes, are skipped. Only personal rating is
retained for default mapping. Date Rated never implies completion or watched date.

Use the maintained Rust csv crate for record parsing plus a strict quote preflight:
csv readers can tolerate malformed quoting, while this ingestion boundary must
reject ambiguous input. SHA-256 fingerprints cover normalized semantic content;
raw CSV is neither retained nor included in errors.
