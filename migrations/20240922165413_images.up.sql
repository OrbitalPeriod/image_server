-- Add up migration script here
DROP TABLE IF EXISTS images;
CREATE TABLE images(
    id SERIAL PRIMARY KEY NOT NULL,
    computed BOOLEAN NOT NULL DEFAULT FALSE,
    file_name VARCHAR(20) NOT NULL
)