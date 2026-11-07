-- Free-text notes on SSH profiles and port forwards (what the host is for,
-- who to ask, how to use the forward). Not secret: never put passwords here.
ALTER TABLE ssh_profiles ADD COLUMN notes TEXT NOT NULL DEFAULT '';
ALTER TABLE port_forwards ADD COLUMN notes TEXT NOT NULL DEFAULT '';
