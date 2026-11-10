-- 0001: leads (waitlist sign-ups now, trial downloads later). RFC-0001, website plan 1 Task 2.
CREATE TABLE lead (
  id             INTEGER PRIMARY KEY,
  kind           TEXT NOT NULL CHECK (kind IN ('waitlist', 'trial')),
  name           TEXT NOT NULL,
  whatsapp       TEXT NOT NULL,          -- normalised +628…
  store_name     TEXT NOT NULL,
  email          TEXT,
  consent_at     TEXT NOT NULL,          -- RFC 3339 UTC
  source         TEXT,                   -- utm_source, when present
  affiliate_code TEXT,                   -- used from plan 4
  status         TEXT NOT NULL DEFAULT 'baru'
                 CHECK (status IN ('baru', 'dihubungi', 'demo', 'beli', 'tidak tertarik')),
  created_at     TEXT NOT NULL,
  UNIQUE (kind, whatsapp)
);
