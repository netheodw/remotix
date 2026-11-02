-- Terminal toolbox: categorized command snippets the user can insert into or
-- run in an open terminal. Seeded with common Linux commands; every row,
-- seeded or not, can be edited or deleted. `{{name}}` in a command is a
-- placeholder the toolbox asks for before inserting it.

CREATE TABLE snippet_categories (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_snippet_categories_name ON snippet_categories(lower(name));

CREATE TABLE snippets (
    id           TEXT PRIMARY KEY,
    category_id  TEXT NOT NULL REFERENCES snippet_categories(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    command      TEXT NOT NULL,
    description  TEXT NOT NULL DEFAULT '',
    sort_order   INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);
CREATE INDEX idx_snippets_category ON snippets(category_id, sort_order);

CREATE TEMP TABLE seed_categories (id TEXT, name TEXT, sort_order INTEGER);
INSERT INTO seed_categories VALUES
    ('builtin-system',   'System',             0),
    ('builtin-disk',     'Disk & Storage',     1),
    ('builtin-process',  'Processes',          2),
    ('builtin-network',  'Network',            3),
    ('builtin-files',    'Files & Search',     4),
    ('builtin-services', 'Services & Logs',    5),
    ('builtin-packages', 'Packages',           6),
    ('builtin-users',    'Users & Permissions', 7),
    ('builtin-docker',   'Docker',             8);

INSERT INTO snippet_categories (id, name, sort_order, created_at, updated_at)
SELECT id, name, sort_order, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
FROM seed_categories;

CREATE TEMP TABLE seed_snippets (category_id TEXT, name TEXT, command TEXT, description TEXT);
INSERT INTO seed_snippets VALUES
    ('builtin-system', 'OS release', 'cat /etc/os-release', 'Distribution name and version'),
    ('builtin-system', 'Kernel', 'uname -a', 'Kernel version and architecture'),
    ('builtin-system', 'Uptime & load', 'uptime', 'How long the host is up and its load average'),
    ('builtin-system', 'Memory', 'free -h', 'RAM and swap usage'),
    ('builtin-system', 'CPU info', 'lscpu', 'CPU model, cores and threads'),
    ('builtin-system', 'Host details', 'hostnamectl', 'Hostname, OS and virtualization'),
    ('builtin-system', 'Logged-in users', 'who', 'Who is logged in right now'),
    ('builtin-system', 'Recent logins', 'last -n 20', 'The last 20 logins'),

    ('builtin-disk', 'Disk usage', 'df -h', 'Free space per filesystem'),
    ('builtin-disk', 'Inode usage', 'df -i', 'Free inodes per filesystem'),
    ('builtin-disk', 'Block devices', 'lsblk', 'Disks and partitions'),
    ('builtin-disk', 'Folder sizes here', 'du -sh * | sort -h', 'Size of each entry in the current directory'),
    ('builtin-disk', 'Largest items', 'du -ahx {{path}} 2>/dev/null | sort -rh | head -20', 'The 20 largest files and folders under a path'),
    ('builtin-disk', 'Files over 100 MB', 'find {{path}} -xdev -type f -size +100M -exec ls -lh {} \; 2>/dev/null', 'Big files under a path'),

    ('builtin-process', 'Top', 'top', 'Live process monitor'),
    ('builtin-process', 'Htop', 'htop', 'Interactive process monitor (if installed)'),
    ('builtin-process', 'Top CPU', 'ps aux --sort=-%cpu | head -15', 'Processes using the most CPU'),
    ('builtin-process', 'Top memory', 'ps aux --sort=-%mem | head -15', 'Processes using the most memory'),
    ('builtin-process', 'Find process', 'pgrep -a {{name}}', 'PIDs and command lines matching a name'),
    ('builtin-process', 'Kill process', 'kill {{pid}}', 'Ask a process to stop (SIGTERM)'),
    ('builtin-process', 'Force kill', 'kill -9 {{pid}}', 'Kill a process immediately (SIGKILL)'),

    ('builtin-network', 'IP addresses', 'ip -br a', 'Interfaces and their addresses'),
    ('builtin-network', 'Routes', 'ip r', 'Routing table'),
    ('builtin-network', 'Listening ports', 'sudo ss -tulpn', 'Open TCP/UDP ports and their processes'),
    ('builtin-network', 'Public IP', 'curl -s https://ifconfig.me; echo', 'The address this host reaches the internet from'),
    ('builtin-network', 'Ping', 'ping -c 4 {{host}}', 'Send four pings to a host'),
    ('builtin-network', 'DNS lookup', 'dig +short {{domain}}', 'Resolve a domain name'),
    ('builtin-network', 'HTTP headers', 'curl -sI {{url}}', 'Response headers of a URL'),
    ('builtin-network', 'Test TCP port', 'nc -zv {{host}} {{port}}', 'Check that a remote port accepts connections'),

    ('builtin-files', 'List all', 'ls -lah', 'Detailed listing including hidden files'),
    ('builtin-files', 'Find by name', 'find {{path}} -name "{{pattern}}" 2>/dev/null', 'Files whose name matches a pattern'),
    ('builtin-files', 'Search text', 'grep -rn "{{text}}" {{path}}', 'Lines containing a text, recursively'),
    ('builtin-files', 'Follow a file', 'tail -f {{file}}', 'Print new lines as they are written'),
    ('builtin-files', 'Recently modified', 'find {{path}} -type f -mmin -60 2>/dev/null', 'Files changed in the last hour'),
    ('builtin-files', 'Create archive', 'tar -czvf {{archive}}.tar.gz {{path}}', 'Compress a path into a .tar.gz'),
    ('builtin-files', 'Extract archive', 'tar -xzvf {{archive}}', 'Extract a .tar.gz here'),

    ('builtin-services', 'Service status', 'systemctl status {{service}}', 'State and recent log of a systemd service'),
    ('builtin-services', 'Restart service', 'sudo systemctl restart {{service}}', 'Restart a systemd service'),
    ('builtin-services', 'Running services', 'systemctl list-units --type=service --state=running', 'All running systemd services'),
    ('builtin-services', 'Failed services', 'systemctl --failed', 'Units that failed to start'),
    ('builtin-services', 'Follow service log', 'journalctl -u {{service}} -f', 'Live log of a systemd service'),
    ('builtin-services', 'Errors this boot', 'journalctl -p err -b', 'Error-level log entries since boot'),
    ('builtin-services', 'Kernel messages', 'sudo dmesg -T | tail -50', 'The last 50 kernel messages'),

    ('builtin-packages', 'Update (apt)', 'sudo apt update && sudo apt upgrade -y', 'Debian/Ubuntu: refresh and upgrade everything'),
    ('builtin-packages', 'Upgradable (apt)', 'apt list --upgradable', 'Debian/Ubuntu: packages with updates'),
    ('builtin-packages', 'Install (apt)', 'sudo apt install -y {{package}}', 'Debian/Ubuntu: install a package'),
    ('builtin-packages', 'Update (dnf)', 'sudo dnf upgrade -y', 'RHEL/Fedora: upgrade everything'),
    ('builtin-packages', 'Install (dnf)', 'sudo dnf install -y {{package}}', 'RHEL/Fedora: install a package'),

    ('builtin-users', 'Who am I', 'id', 'Current user, its groups and ids'),
    ('builtin-users', 'Sudo rights', 'sudo -l', 'What the current user may run with sudo'),
    ('builtin-users', 'Login users', 'getent passwd | awk -F: ''$3 >= 1000 {print $1}''', 'Regular (non-system) accounts'),
    ('builtin-users', 'Add user', 'sudo useradd -m -s /bin/bash {{user}}', 'Create a user with a home directory'),
    ('builtin-users', 'Change owner', 'sudo chown -R {{user}}:{{group}} {{path}}', 'Recursively change ownership'),
    ('builtin-users', 'Change mode', 'chmod {{mode}} {{path}}', 'Change permissions, e.g. 644 or 755'),

    ('builtin-docker', 'Running containers', 'docker ps', 'Containers that are up'),
    ('builtin-docker', 'All containers', 'docker ps -a', 'Including stopped containers'),
    ('builtin-docker', 'Container log', 'docker logs -f --tail 100 {{container}}', 'Follow the last 100 log lines'),
    ('builtin-docker', 'Shell in container', 'docker exec -it {{container}} sh', 'Open a shell inside a container'),
    ('builtin-docker', 'Resource usage', 'docker stats --no-stream', 'CPU and memory per container'),
    ('builtin-docker', 'Disk usage', 'docker system df', 'Space used by images, containers and volumes'),
    ('builtin-docker', 'Compose up', 'docker compose up -d', 'Start the compose project in this directory'),
    ('builtin-docker', 'Compose logs', 'docker compose logs -f --tail 100', 'Follow the compose project logs');

INSERT INTO snippets (id, category_id, name, command, description, sort_order, created_at, updated_at)
SELECT
    'builtin-snippet-' || rowid,
    category_id,
    name,
    command,
    description,
    rowid,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
FROM seed_snippets;

DROP TABLE seed_categories;
DROP TABLE seed_snippets;
