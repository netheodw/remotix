-- Optional name for a local/dynamic forward (e.g. ifg-portainer.local).
-- Remotix maps it to the forward's bind address in the OS hosts file.
ALTER TABLE port_forwards ADD COLUMN local_hostname TEXT NULL;
