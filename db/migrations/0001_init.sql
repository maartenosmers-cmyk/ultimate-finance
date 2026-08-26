-- 0001_init.sql — core domain model.
--
-- Conventions:
--   * Money: BIGINT minor units (cents). Signed: income positive, expenses negative.
--     Account balances are signed (credit cards / loans carry negative balances).
--   * IDs: UUIDv4 (gen_random_uuid) for external-facing rows; bigserial only
--     where ordering matters and the id is internal.
--   * Timestamps: TIMESTAMPTZ, UTC. Dates that represent *bank* days are DATE.
--   * Every mutable table has created_at/updated_at; triggers keep updated_at fresh.

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ---------------------------------------------------------------- identity --

CREATE TABLE households (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name        text NOT NULL,
    currency    char(3) NOT NULL DEFAULT 'USD',
    fiscal_month_start smallint NOT NULL DEFAULT 1 CHECK (fiscal_month_start BETWEEN 1 AND 28),
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE users (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    email         text NOT NULL,
    display_name  text NOT NULL,
    -- nullable: users may be passkey-only
    password_hash text,
    avatar_url    text,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX users_email_key ON users (lower(email));

CREATE TYPE member_role AS ENUM ('owner', 'member', 'advisor');
CREATE TYPE member_status AS ENUM ('invited', 'active', 'revoked');

CREATE TABLE household_members (
    household_id uuid NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    user_id      uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role         member_role NOT NULL DEFAULT 'member',
    status       member_status NOT NULL DEFAULT 'active',
    invited_by   uuid REFERENCES users(id),
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (household_id, user_id)
);

-- ------------------------------------------------------------- connections --

CREATE TABLE institutions (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    aggregator   text NOT NULL CHECK (aggregator IN ('plaid', 'teller', 'mx', 'manual')),
    external_id  text NOT NULL,
    name         text NOT NULL,
    logo_url     text,
    UNIQUE (aggregator, external_id)
);

CREATE TABLE connections (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id     uuid NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    institution_id   uuid NOT NULL REFERENCES institutions(id),
    -- provider credentials live in a KMS-encrypted column, never plaintext
    credentials_enc  bytea,
    status           text NOT NULL DEFAULT 'pending'
                     CHECK (status IN ('pending', 'connected', 'requires_reauth', 'error', 'disconnected')),
    last_synced_at   timestamptz,
    sync_cursor      jsonb,
    error_code       text,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX connections_household_idx ON connections (household_id);

CREATE TYPE account_type AS ENUM
  ('checking', 'savings', 'credit_card', 'brokerage', 'retirement', 'loan',
   'mortgage', 'property', 'vehicle', 'cash', 'other');
CREATE TYPE visibility AS ENUM ('all_members', 'partner_only', 'private');

CREATE TABLE accounts (
    id                      uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id            uuid NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    connection_id           uuid REFERENCES connections(id) ON DELETE SET NULL,
    type                    account_type NOT NULL,
    subtype                 text,
    name                    text NOT NULL,
    mask                    text,
    currency                char(3) NOT NULL DEFAULT 'USD',
    -- signed; assets > 0, liabilities < 0
    current_balance_minor   bigint NOT NULL DEFAULT 0,
    available_balance_minor bigint,
    limit_minor             bigint,
    is_manual               boolean NOT NULL DEFAULT false,
    visibility              visibility NOT NULL DEFAULT 'all_members',
    opened_on               date,
    closed_on               date,
    metadata                jsonb NOT NULL DEFAULT '{}',
    created_at              timestamptz NOT NULL DEFAULT now(),
    updated_at              timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX accounts_household_idx ON accounts (household_id);
CREATE INDEX accounts_connection_idx ON accounts (connection_id);

CREATE TABLE balance_snapshots (
    account_id    uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    at            timestamptz NOT NULL,
    balance_minor bigint NOT NULL,
    source        text NOT NULL DEFAULT 'sync' CHECK (source IN ('sync', 'manual', 'import', 'computed')),
    created_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (account_id, at)
);

-- ------------------------------------------------------------ transactions --

CREATE TYPE category_kind AS ENUM ('income', 'expense', 'transfer', 'group');

CREATE TABLE categories (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid REFERENCES households(id) ON DELETE CASCADE, -- null = system default
    parent_id    uuid REFERENCES categories(id) ON DELETE CASCADE,
    name         text NOT NULL,
    kind         category_kind NOT NULL DEFAULT 'expense',
    color        text,
    icon         text,
    sort_order   integer NOT NULL DEFAULT 0,
    is_system    boolean NOT NULL DEFAULT false,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX categories_household_idx ON categories (household_id);

CREATE TABLE category_rules (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    priority     integer NOT NULL DEFAULT 100,
    -- {"field":"merchant|description|amount","op":"contains|regex|eq|gt|lt","value":...}
    match_spec   jsonb NOT NULL,
    category_id  uuid NOT NULL REFERENCES categories(id),
    active       boolean NOT NULL DEFAULT true,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TYPE review_state AS ENUM ('unreviewed', 'reviewed', 'needs_attention');
CREATE TYPE tx_source AS ENUM ('manual', 'import', 'aggregate');

CREATE TABLE transactions (
    id                  uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id        uuid NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    account_id          uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    external_id         text,
    posted_on           date NOT NULL,
    amount_minor        bigint NOT NULL,
    merchant_raw        text,
    merchant_normalized text,
    description         text,
    category_id         uuid REFERENCES categories(id) ON DELETE SET NULL,
    review_state        review_state NOT NULL DEFAULT 'unreviewed',
    notes               text,
    is_transfer         boolean NOT NULL DEFAULT false,
    transfer_group_id   uuid,
    recurring_id        uuid,
    source              tx_source NOT NULL DEFAULT 'manual',
    import_batch_id     uuid,
    -- content hash for dedup on re-import / webhook replay
    dedup_hash          text,
    metadata            jsonb NOT NULL DEFAULT '{}',
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    UNIQUE (account_id, external_id)
);
CREATE INDEX tx_household_date_idx ON transactions (household_id, posted_on DESC);
CREATE INDEX tx_account_date_idx ON transactions (account_id, posted_on DESC);
CREATE INDEX tx_category_idx ON transactions (household_id, category_id);
CREATE INDEX tx_dedup_idx ON transactions (account_id, dedup_hash);

CREATE TABLE transaction_splits (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    transaction_id uuid NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
    position       smallint NOT NULL DEFAULT 0,
    amount_minor   bigint NOT NULL,
    category_id    uuid REFERENCES categories(id) ON DELETE SET NULL,
    memo           text
);
CREATE INDEX splits_tx_idx ON transaction_splits (transaction_id);

CREATE TABLE attachments (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id   uuid NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    transaction_id uuid REFERENCES transactions(id) ON DELETE CASCADE,
    mime_type      text NOT NULL,
    storage_key    text NOT NULL,
    ocr_text       text,
    created_at     timestamptz NOT NULL DEFAULT now()
);

-- --------------------------------------------------------------- recurring --

CREATE TYPE cadence AS ENUM ('weekly', 'biweekly', 'monthly', 'quarterly', 'annual', 'irregular');
CREATE TYPE recurring_status AS ENUM ('active', 'paused', 'ended');

CREATE TABLE recurrings (
    id                    uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id          uuid NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    account_id            uuid REFERENCES accounts(id) ON DELETE SET NULL,
    category_id           uuid REFERENCES categories(id) ON DELETE SET NULL,
    merchant_normalized   text NOT NULL,
    expected_amount_minor bigint NOT NULL,
    last_amount_minor     bigint,
    cadence               cadence NOT NULL,
    next_expected_on      date,
    confidence            real NOT NULL DEFAULT 0.5 CHECK (confidence BETWEEN 0 AND 1),
    status                recurring_status NOT NULL DEFAULT 'active',
    detected_at           timestamptz NOT NULL DEFAULT now(),
    updated_at            timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX recurrings_household_idx ON recurrings (household_id, status);

-- ---------------------------------------------------------- budget & goals --

CREATE TYPE budget_line_kind AS ENUM ('fixed', 'flexible', 'rolling');

CREATE TABLE budgets (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id uuid NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    name         text NOT NULL,
    period       text NOT NULL DEFAULT 'monthly',
    starts_on    date NOT NULL,
    archived     boolean NOT NULL DEFAULT false,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE budget_lines (
    budget_id         uuid NOT NULL REFERENCES budgets(id) ON DELETE CASCADE,
    category_id       uuid NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
    amount_minor      bigint NOT NULL,
    kind              budget_line_kind NOT NULL DEFAULT 'flexible',
    rollover_enabled  boolean NOT NULL DEFAULT false,
    PRIMARY KEY (budget_id, category_id)
);

CREATE TYPE goal_status AS ENUM ('active', 'achieved', 'paused', 'abandoned');

CREATE TABLE goals (
    id                  uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id        uuid NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    name                text NOT NULL,
    target_amount_minor bigint NOT NULL CHECK (target_amount_minor > 0),
    target_on           date,
    monthly_plan_minor  bigint,
    account_ids         uuid[] NOT NULL DEFAULT '{}',
    status              goal_status NOT NULL DEFAULT 'active',
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE goal_contributions (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    goal_id        uuid NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
    transaction_id uuid REFERENCES transactions(id) ON DELETE SET NULL,
    amount_minor   bigint NOT NULL,
    at             timestamptz NOT NULL DEFAULT now(),
    note           text
);
CREATE INDEX goal_contrib_goal_idx ON goal_contributions (goal_id, at);

-- --------------------------------------------- investments & tax awareness --

CREATE TABLE securities (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    symbol      text NOT NULL UNIQUE,
    name        text NOT NULL,
    asset_class text NOT NULL DEFAULT 'equity',
    isin        text
);

CREATE TABLE security_prices (
    security_id uuid NOT NULL REFERENCES securities(id) ON DELETE CASCADE,
    at          timestamptz NOT NULL,
    price       numeric(20, 8) NOT NULL,
    PRIMARY KEY (security_id, at)
);

CREATE TABLE holdings (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id        uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    security_id       uuid NOT NULL REFERENCES securities(id) ON DELETE CASCADE,
    quantity          numeric(24, 8) NOT NULL,
    cost_basis_minor  bigint NOT NULL DEFAULT 0,
    as_of             date NOT NULL,
    UNIQUE (account_id, security_id)
);

-- Tax lots exist from day one so planning projections can model withdrawal
-- ordering (FIFO/LIFO/HIFO/specific-ID) without a painful backfill later.
CREATE TYPE lot_method AS ENUM ('fifo', 'lifo', 'hifo', 'spec_id');
CREATE TABLE tax_lots (
    id                 uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id         uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    security_id        uuid NOT NULL REFERENCES securities(id) ON DELETE CASCADE,
    acquired_on        date NOT NULL,
    quantity           numeric(24, 8) NOT NULL,
    cost_basis_minor   bigint NOT NULL,
    closed_quantity    numeric(24, 8) NOT NULL DEFAULT 0,
    method             lot_method NOT NULL DEFAULT 'fifo'
);

-- ---------------------------------------------------------------- scenarios --

CREATE TABLE scenarios (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    household_id  uuid NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    name          text NOT NULL,
    description   text,
    -- normalized snapshot of the financial state this scenario branches from
    base_snapshot jsonb NOT NULL,
    created_by    uuid REFERENCES users(id),
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX scenarios_household_idx ON scenarios (household_id);

CREATE TABLE scenario_runs (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    scenario_id    uuid NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
    engine_version text NOT NULL,
    inputs         jsonb NOT NULL,
    outputs        jsonb NOT NULL,
    created_at     timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX runs_scenario_idx ON scenario_runs (scenario_id, created_at DESC);

-- ------------------------------------------------------- platform plumbing --

-- Idempotent webhook ingestion: providers retry; we must not double-process.
CREATE TABLE webhook_events (
    id           bigserial PRIMARY KEY,
    aggregator   text NOT NULL,
    event_id     text NOT NULL,
    payload      jsonb NOT NULL,
    status       text NOT NULL DEFAULT 'received'
                 CHECK (status IN ('received', 'processed', 'failed', 'ignored')),
    received_at  timestamptz NOT NULL DEFAULT now(),
    processed_at timestamptz,
    UNIQUE (aggregator, event_id)
);

CREATE TABLE audit_log (
    id            bigserial PRIMARY KEY,
    household_id  uuid NOT NULL,
    actor_user_id uuid,
    action        text NOT NULL,
    entity_type   text NOT NULL,
    entity_id     uuid,
    diff          jsonb,
    at            timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX audit_household_idx ON audit_log (household_id, at DESC);

-- ------------------------------------------------------------ housekeeping --

CREATE OR REPLACE FUNCTION touch_updated_at() RETURNS trigger AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY[
        'households','users','household_members','connections','accounts',
        'categories','category_rules','transactions','recurrings','budgets',
        'goals','scenarios'
    ] LOOP
        EXECUTE format(
            'CREATE TRIGGER %I_touch BEFORE UPDATE ON %I FOR EACH ROW EXECUTE FUNCTION touch_updated_at()',
            t, t
        );
    END LOOP;
END $$;
