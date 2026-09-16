# Primary evidence reviewed 2026-09-16

- https://neodb.net/api/ : register /api/v1/apps, authorize /oauth/authorize, exchange /oauth/token, authenticate /api/me. Explicitly request read rather than the example's read+write.
- https://github.com/neodb-social/neodb/blob/main/takahe/api/views/oauth.py : code exchange validates client secret and redirect_uri; state is echoed; do not claim unverified PKCE support.
- https://github.com/neodb-social/neodb/blob/main/neodb/catalog/apis.py : catalog/fetch enqueues fetch for missing records; exclude it from preview adapter.
- https://github.com/neodb-social/neodb/blob/main/neodb/journal/apis/shelf.py : mark POST replaces unspecified fields and requires shelf/visibility; no writes in this milestone. Detail GET may redirect merged items; constrain redirect origin.
- https://help.imdb.com/article/imdb/track-movies-tv/faq-for-imdb-ratings/G67Y87TFYYP6TWAV : official CSV export; personal ratings 1..10; rating does not imply watched completion date.
- https://docs.rs/keyring/3.6.3/keyring/ : explicit native features are mandatory (no defaults; unsupported configurations otherwise select a mock backend). Use Windows/apple native and Linux sync-secret-service + crypto-rust. Linux requires DBus library/headers unless vendored; serialize process-local calls because native stores have concurrency caveats. Version 3.6.3 is deliberately used, not an unreviewed major upgrade.
