# Deploying stb-web

How to run sistemtokobangunan.com on one server: the `stb-web` binary under systemd, Caddy in
front for HTTPS, and a daily database backup by cron (RFC-0001, ADR-0003 in the wiki).

| File | Installed as |
|---|---|
| `stb-web.service` | `/etc/systemd/system/stb-web.service` |
| `env.example` | `/etc/stb-web/env` (owner `stb`, mode 0600, real values filled in) |
| `Caddyfile` | `/etc/caddy/Caddyfile` |
| `stb-web-backup.cron` | `/etc/cron.d/stb-web-backup` (owner root, mode 0644, no `.cron` suffix) |

These files target **any x86-64 Linux VPS with systemd and Caddy**. They have not yet been run
on a Linux server: the unit, Caddyfile and cron syntax are unverified until the first
deployment.

## Before going live (gate)

Do not deploy publicly until the owner has:

1. **chosen the VPS** provider and region (wiki OQ-128);
2. **confirmed the domain** `sistemtokobangunan.com`: registered, and who holds it (RFC-0001
   open question 10);
3. **reviewed the privacy text** at `/privasi` (`templates/privacy.html`).

## Server prerequisites

- x86-64 Linux with systemd, cron and the `tzdata` package (for `Asia/Jakarta`); `sqlite3` is
  useful for checks.
- [Caddy](https://caddyserver.com/docs/install) installed as a systemd service from its
  official packages.
- DNS `A` (and `AAAA` if the server has IPv6) records for `sistemtokobangunan.com` and
  `www.sistemtokobangunan.com` pointing at the server.
- Ports 80 and 443 open to the internet (Caddy needs both for certificates). Port 8080 stays
  closed: `stb-web` listens on `127.0.0.1` only.

## 1. Build the binary for Linux x86-64

The release binary must be a **Linux x86-64** build. We develop on macOS, and a macOS build
does not run on the server. Either:

- **build on the server** (or any x86-64 Linux machine): install Rust with
  [rustup](https://rustup.rs) and a C compiler (SQLite is compiled in; on Debian/Ubuntu
  `apt install build-essential`), copy the repository there, then

  ```sh
  cargo build --release          # gives target/release/stb-web
  ```

- or **cross-compile** from macOS. This has **not been tried**; do not rely on it until
  someone has built and run a binary that way.

## 2. User and directories (once)

```sh
sudo useradd --system --home-dir /var/lib/stb-web --shell /usr/sbin/nologin stb
sudo install -d -o stb -g stb -m 0750 /var/lib/stb-web /var/backups/stb-web
sudo install -d -o root -g root -m 0755 /opt/stb-web /opt/stb-web/static /etc/stb-web
```

`/var/lib/stb-web` must exist before the service starts: the unit lists it in
`ReadWritePaths=`. The service itself cannot write `/var/backups/stb-web` (only the cron job,
outside the sandbox, writes there), so a compromised web process cannot delete the backups.

## 3. Binary and static files

From the repository root, after the build:

```sh
sudo install -o root -g root -m 0755 target/release/stb-web /opt/stb-web/stb-web
sudo cp -R static/. /opt/stb-web/static/
sudo chown -R root:root /opt/stb-web/static
```

The templates are compiled into the binary; only `static/` is copied. `stb-web serve` finds it
through the unit's `WorkingDirectory=/opt/stb-web`.

## 4. Environment

```sh
sudo install -o stb -g stb -m 0600 deploy/env.example /etc/stb-web/env
sudo -e /etc/stb-web/env
```

Fill in real values. `STB_WHATSAPP_NUMBER` is **digits only, country code first, no `+`**
(e.g. `6281234567890`), because it is used in `https://wa.me/<number>` links. The server
checks it at start-up: it must be `62` followed by 8 to 13 digits, otherwise `stb-web serve`
logs "environment variable STB_WHATSAPP_NUMBER has an invalid value" (see `journalctl -u
stb-web`) and exits, so the placeholder from `env.example` is refused too.
`STB_CONTACT_EMAIL` is the public contact address shown as a `mailto:` link on the home and
privacy pages; it is checked the same way (one `@` with text on both sides, a `.` after the
`@`, no spaces, at most 254 characters), so its placeholder is refused as well. `STB_BASE_URL` has
no trailing slash. Keep `KEY=value` lines with no quotes or spaces: systemd and the cron job's
`/bin/sh` both read this file.

The file is owned by `stb` because the backup cron job runs as `stb` and reads
`STB_DATABASE_PATH` from it. It holds no password today; keep it 0600 anyway, since later
plans add secrets here.

## 5. The service

```sh
sudo install -o root -g root -m 0644 deploy/stb-web.service /etc/systemd/system/stb-web.service
sudo systemd-analyze verify /etc/systemd/system/stb-web.service
sudo systemctl daemon-reload
sudo systemctl enable --now stb-web
systemctl status stb-web
curl -s http://127.0.0.1:8080/healthz      # expect: ok
journalctl -u stb-web -n 50
```

`serve` creates the database and applies migrations on first start.

## 6. Caddy (HTTPS)

```sh
sudo install -o root -g root -m 0644 deploy/Caddyfile /etc/caddy/Caddyfile
caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile
sudo systemctl reload caddy
curl -s https://sistemtokobangunan.com/healthz                  # expect: ok
curl -sI https://www.sistemtokobangunan.com/ | head -n 3        # expect: 301 to the bare domain
```

The site block compresses responses (`encode zstd gzip`) and sends `X-Content-Type-Options:
nosniff` and `Referrer-Policy: strict-origin-when-cross-origin`. It does **not** send HSTS
(`Strict-Transport-Security`) yet, because the domain is not confirmed: once HTTPS is confirmed
working on the final domain, HSTS can be added to the `header` block (for example
`Strict-Transport-Security "max-age=31536000"`).

## 7. Daily backup

```sh
sudo install -o root -g root -m 0644 deploy/stb-web-backup.cron /etc/cron.d/stb-web-backup
timedatectl                                   # the cron line assumes the clock is UTC
```

The job runs at 02:00 WIB (19:00 UTC) as `stb`, writes
`/var/backups/stb-web/stb-YYYYMMDD.db`, and then deletes backups 14 days old or more. If
`timedatectl` shows a time zone other than UTC, change the schedule as the file's comment says.
Check that it works by running the same command once by hand with a test file name:

```sh
sudo -u stb sh -c 'set -a && . /etc/stb-web/env && set +a && /opt/stb-web/stb-web backup /var/backups/stb-web/manual-test.db'
sudo -u stb sqlite3 /var/backups/stb-web/manual-test.db 'PRAGMA integrity_check; SELECT count(*) FROM lead;'
sudo rm -f /var/backups/stb-web/manual-test.db /var/backups/stb-web/manual-test.db-wal /var/backups/stb-web/manual-test.db-shm
```

A backup is a WAL-mode database, so opening it with `sqlite3` may leave `-wal` and `-shm`
files beside it; delete them with the file. To inspect a nightly backup, copy it elsewhere
first rather than opening it in place.

The next morning, `journalctl -t stb-web-backup` shows the night's run.

**Off-site copy: the owner's step.** The backups sit on the same VPS as the database, so they
do not survive losing the server. The owner copies `/var/backups/stb-web/` off the server
regularly (for example a daily `scp`/`rsync` pull to another machine, or the VPS provider's
snapshot or object storage). The backups hold leads' names, WhatsApp numbers and emails:
keep the off-site copy private.

## 8. Live check

1. Open `https://sistemtokobangunan.com/` on a phone and sign up with a test name.
2. On the server, export and look for the row:

   ```sh
   sudo -u stb sh -c 'set -a && . /etc/stb-web/env && set +a && /opt/stb-web/stb-web waitlist-export /var/lib/stb-web/check.csv'
   sudo cat /var/lib/stb-web/check.csv
   sudo rm /var/lib/stb-web/check.csv
   ```

3. Delete the test row. There is no command for this yet; use `sqlite3` while the service runs
   (SQLite WAL allows it), with the test row's stored WhatsApp number (`+628…`):

   ```sh
   sudo -u stb sqlite3 /var/lib/stb-web/stb.db "DELETE FROM lead WHERE kind = 'waitlist' AND whatsapp = '+628XXXXXXXXXX';"
   ```

## Routine checks (weekly)

The backup cron job's failures go only to the journal; nobody is notified. Once a week:

```sh
ls -lt /var/backups/stb-web | head                      # newest stb-YYYYMMDD.db is from last night
journalctl -t stb-web-backup --since "-7 days"          # look for errors from the backup runs
```

If last night's file is missing or the journal shows errors, run the manual backup check in
step 7 and fix the cause before the 14-day retention deletes the last good backups.

## Updating to a new version

```sh
# build the new binary first (step 1), then:
sudo -u stb sh -c 'set -a && . /etc/stb-web/env && set +a && /opt/stb-web/stb-web backup /var/backups/stb-web/pre-update-$(date +%Y%m%d%H%M).db'
sudo install -o root -g root -m 0755 target/release/stb-web /opt/stb-web/stb-web.new
sudo mv /opt/stb-web/stb-web.new /opt/stb-web/stb-web
sudo cp -R static/. /opt/stb-web/static/
# add any new STB_* variables from deploy/env.example to /etc/stb-web/env first (this release:
# STB_CONTACT_EMAIL); `stb-web serve` exits at start-up if a required one is missing
sudo -e /etc/stb-web/env
sudo systemctl restart stb-web
curl -s http://127.0.0.1:8080/healthz         # expect: ok
```

Migrations run when `serve` starts. `pre-update-*.db` files are not deleted by the cron job
(it only matches `stb-*.db`); remove them by hand once the update is confirmed.

## Restoring from a backup

Set `BACKUP` to the file to restore, then run the block as one command. It runs under
`sudo sh -e`, so it stops at the first failing step. It checks the backup exists before
touching anything, stops the service, moves the live `stb.db` and its `-wal` and `-shm` files
aside under a timestamped name (nothing is deleted), installs the backup and starts the service.

```sh
BACKUP=/var/backups/stb-web/stb-YYYYMMDD.db
sudo sh -e -c '
  backup="$1"
  dir=/var/lib/stb-web
  aside="$dir/stb.db.broken-$(date +%Y%m%d%H%M%S)"
  test -f "$backup"
  systemctl stop stb-web
  if [ -e "$dir/stb.db" ]; then mv "$dir/stb.db" "$aside"; fi
  if [ -e "$dir/stb.db-wal" ]; then mv "$dir/stb.db-wal" "$aside-wal"; fi
  if [ -e "$dir/stb.db-shm" ]; then mv "$dir/stb.db-shm" "$aside-shm"; fi
  install -o stb -g stb -m 0640 "$backup" "$dir/stb.db"
  systemctl start stb-web
  echo "restored $backup; old files kept as $aside*"
' restore "$BACKUP"
curl -s http://127.0.0.1:8080/healthz         # expect: ok
```

If the block stops part-way, the service stays stopped and the old files are wherever the
last successful step left them (still `stb.db*`, or moved to `stb.db.broken-<time>*`); move
them back or finish the steps by hand, then start the service. The moved files keep SQLite's
naming (`<name>`, `<name>-wal`), so `sqlite3` opens the old database with its WAL. Sign-ups
made after that backup are lost unless they are still in the `stb.db.broken-<time>` files.
