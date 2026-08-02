-- Initial schema mirroring the Django models from reseller-control-center
-- PostgreSQL UUID PKs, same table/column names for drop-in parity.

CREATE EXTENSION IF NOT EXISTS "pgcrypto";

CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username VARCHAR(150) NOT NULL UNIQUE,
    email VARCHAR(254) NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role VARCHAR(10) NOT NULL DEFAULT 'RESELLER',
    credit_balance NUMERIC(12, 2) NOT NULL DEFAULT 0.00,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    uuid UUID NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    last_login_ip INET,
    whatsapp_phone VARCHAR(20) NOT NULL DEFAULT '',
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    is_staff BOOLEAN NOT NULL DEFAULT FALSE,
    is_superuser BOOLEAN NOT NULL DEFAULT FALSE,
    last_login TIMESTAMPTZ,
    date_joined TIMESTAMPTZ NOT NULL DEFAULT now(),
    first_name VARCHAR(150) NOT NULL DEFAULT '',
    last_name VARCHAR(150) NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_users_role ON users (role);

CREATE TABLE IF NOT EXISTS providers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL UNIQUE,
    slug VARCHAR(100) NOT NULL UNIQUE,
    adapter_key VARCHAR(20) NOT NULL DEFAULT 'mock',
    api_endpoint VARCHAR(500) NOT NULL DEFAULT '',
    api_token BYTEA,
    extra_config JSONB NOT NULL DEFAULT '{}'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL UNIQUE,
    slug VARCHAR(100) NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    image VARCHAR(100),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS products (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    category_id UUID REFERENCES categories(id) ON DELETE RESTRICT,
    provider_id UUID REFERENCES providers(id) ON DELETE RESTRICT,
    description TEXT NOT NULL DEFAULT '',
    external_pack_id INTEGER,
    duration_months INTEGER,
    price_in_credits NUMERIC(10, 2),
    image VARCHAR(100),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    is_manual BOOLEAN NOT NULL DEFAULT FALSE,
    credential_type VARCHAR(20),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_products_is_active ON products (is_active);
CREATE INDEX IF NOT EXISTS idx_products_is_active_category ON products (is_active, category_id);
CREATE INDEX IF NOT EXISTS idx_products_category ON products (category_id);

CREATE TABLE IF NOT EXISTS product_variants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    duration_months INTEGER,
    is_lifetime BOOLEAN NOT NULL DEFAULT FALSE,
    external_pack_id INTEGER,
    price_in_credits NUMERIC(10, 2) NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_variant_product UNIQUE (product_id, external_pack_id, duration_months)
);

CREATE TABLE IF NOT EXISTS orders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    uuid UUID NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    reseller_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE RESTRICT,
    variant_id UUID REFERENCES product_variants(id) ON DELETE RESTRICT,
    quantity INTEGER NOT NULL,
    unit_price_at_purchase NUMERIC(10, 2) NOT NULL,
    product_name_at_purchase VARCHAR(100) NOT NULL,
    total_credits NUMERIC(12, 2) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING',
    failure_reason TEXT,
    idempotency_key VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ,
    CONSTRAINT uq_reseller_idempotency UNIQUE (reseller_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_orders_reseller_created ON orders (reseller_id, created_at DESC);

CREATE TABLE IF NOT EXISTS credentials (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    external_username VARCHAR(100) NOT NULL,
    streaming_username VARCHAR(100),
    encrypted_password BYTEA NOT NULL,
    dns_domain VARCHAR(255) NOT NULL,
    m3u_url VARCHAR(500) NOT NULL DEFAULT '',
    data JSONB NOT NULL DEFAULT '{}'::jsonb,
    expires_at TIMESTAMPTZ,
    is_revoked BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_credentials_order ON credentials (order_id);
CREATE INDEX IF NOT EXISTS idx_credentials_streaming_username ON credentials (streaming_username);
CREATE INDEX IF NOT EXISTS idx_credentials_order_revoked ON credentials (order_id, is_revoked);

CREATE TABLE IF NOT EXISTS credit_transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reseller_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    delta NUMERIC(12, 2) NOT NULL,
    balance_after NUMERIC(12, 2) NOT NULL,
    actor VARCHAR(20) NOT NULL,
    reason VARCHAR(255) NOT NULL,
    reference_order_id UUID REFERENCES orders(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_credit_transactions_reseller ON credit_transactions (reseller_id, created_at DESC);

CREATE TABLE IF NOT EXISTS idempotency_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reseller_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key VARCHAR(255) NOT NULL,
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_reseller_key UNIQUE (reseller_id, key)
);

CREATE TABLE IF NOT EXISTS quarantined_credentials (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id UUID REFERENCES orders(id) ON DELETE SET NULL,
    username VARCHAR(100) NOT NULL,
    encrypted_password BYTEA NOT NULL,
    provider_response JSONB NOT NULL DEFAULT '{}'::jsonb,
    reason TEXT NOT NULL,
    resolved BOOLEAN NOT NULL DEFAULT FALSE,
    resolved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_quarantined_resolved ON quarantined_credentials (resolved);
