-- Support provider catalog sync: unique (provider_id, external_pack_id) for
-- products sourced from a provider adapter (manual products keep NULL pack ids).
CREATE UNIQUE INDEX IF NOT EXISTS uq_products_provider_pack
    ON products (provider_id, external_pack_id)
    WHERE external_pack_id IS NOT NULL;
