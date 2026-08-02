-- ManualProductCredential (Django dashboard app): stock of manually-managed
-- activation codes / username+password credentials assigned to orders.
CREATE TABLE IF NOT EXISTS manual_credentials (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    uuid UUID NOT NULL DEFAULT gen_random_uuid(),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    variant_id UUID REFERENCES product_variants(id) ON DELETE SET NULL,
    credential_type VARCHAR(20) NOT NULL DEFAULT 'username_password',
    username VARCHAR(150),
    password VARCHAR(255),
    code VARCHAR(150),
    notes TEXT,
    status VARCHAR(20) NOT NULL DEFAULT 'available',
    assigned_to UUID REFERENCES users(id) ON DELETE SET NULL,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    assigned_at TIMESTAMPTZ,
    used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_manual_cred_status CHECK (status IN ('available', 'used', 'expired'))
);

CREATE INDEX IF NOT EXISTS idx_manual_cred_product_status ON manual_credentials (product_id, status);
CREATE INDEX IF NOT EXISTS idx_manual_cred_product_variant_status ON manual_credentials (product_id, variant_id, status);
CREATE INDEX IF NOT EXISTS idx_manual_cred_assigned_to_status ON manual_credentials (assigned_to, status);
CREATE INDEX IF NOT EXISTS idx_manual_cred_status ON manual_credentials (status);
