-- Immutable compact explicit manual catalog choices. URLs and prior evidence are excluded.
ALTER TABLE preview_observations
ADD COLUMN manual_selection TEXT NULL
    CHECK (
        manual_selection IS NULL
        OR length(CAST(manual_selection AS BLOB)) BETWEEN 1 AND 512
    );

CREATE TRIGGER preview_observations_resolution_input_insert
BEFORE INSERT ON preview_observations
WHEN NEW.search_query IS NOT NULL AND NEW.manual_selection IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'preview resolution input conflict');
END;

CREATE TRIGGER preview_observations_resolution_input_update
BEFORE UPDATE OF search_query, manual_selection ON preview_observations
WHEN NEW.search_query IS NOT NULL AND NEW.manual_selection IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'preview resolution input conflict');
END;
