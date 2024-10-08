-- Add up migration script here
DROP TABLE IF EXISTS images;
CREATE TABLE images(
    image_identifier UUID NOT NULL,
    computed BOOLEAN NOT NULL DEFAULT FALSE,
    image_format VARCHAR(4) NOT NULL,
    expires_at timestamptz NOT NULL,
    PRIMARY KEY(image_identifier, image_format)
)