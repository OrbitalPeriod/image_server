-- Add up migration script here
DROP TABLE IF EXISTS images;
CREATE TABLE images(
    image_identifier UUID PRIMARY KEY NOT NULL,
    computed BOOLEAN NOT NULL DEFAULT FALSE
)