-- Immutable normalized explicit catalog-search inputs. NULL remains native-only.
ALTER TABLE preview_observations
ADD COLUMN search_query TEXT NULL
    CHECK (
        search_query IS NULL
        OR length(CAST(search_query AS BLOB)) BETWEEN 1 AND 512
    );
