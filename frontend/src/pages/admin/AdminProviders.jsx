import { useState, useEffect } from 'react';
import AdminLayout from '../../components/AdminLayout';
import { useToast } from '../../context/ToastContext';
import api from '../../lib/axios';

const PAGE_SIZE = 10;

function slugify(name) {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/(^-|-$)/g, '');
}

function FieldInput({ field, value, onChange }) {
  const common = {
    className: 'admin-input',
    value: value ?? '',
    onChange: e => onChange(e.target.value),
  };
  if (field.type === 'secret') {
    return <input type="password" placeholder="••••" {...common} />;
  }
  if (field.type === 'url') {
    return <input type="url" placeholder="https://..." {...common} />;
  }
  if (field.type === 'number') {
    return <input type="number" step="any" {...common} />;
  }
  if (field.type === 'select') {
    const options = Array.isArray(field.options) ? field.options : [];
    return (
      <select className="admin-select" value={value ?? ''} onChange={e => onChange(e.target.value)}>
        <option value="">Select...</option>
        {options.map((opt, i) => {
          const v = typeof opt === 'object' ? opt.value : opt;
          const l = typeof opt === 'object' ? opt.label : opt;
          return <option key={i} value={v}>{l}</option>;
        })}
      </select>
    );
  }
  return <input type="text" {...common} />;
}

export default function AdminProviders() {
  const { addToast } = useToast();
  const [providers, setProviders] = useState([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');
  const [page, setPage] = useState(1);
  const [formOpen, setFormOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState(null);
  const [form, setForm] = useState({
    id: null,
    name: '',
    slug: '',
    adapter_key: '',
    api_endpoint: '',
    extra_config: '{}',
    provider_config: {},
    is_active: false,
    api_token: '',
    has_token: false,
  });

  const fetchProviders = () => {
    api.get('/dashboard/providers/')
      .then(res => setProviders(res.data))
      .catch(() => addToast('Failed to load providers.', 'error'))
      .finally(() => setLoading(false));
  };

  useEffect(() => { fetchProviders(); }, []);

  const filtered = providers.filter(p =>
    !search ||
    p.name.toLowerCase().includes(search.toLowerCase()) ||
    p.slug.toLowerCase().includes(search.toLowerCase()) ||
    p.adapter_key.toLowerCase().includes(search.toLowerCase())
  );
  const totalPages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const pageRows = filtered.slice((page - 1) * PAGE_SIZE, page * PAGE_SIZE);

  const openCreate = () => {
    setForm({
      id: null, name: '', slug: '', adapter_key: '', api_endpoint: '',
      extra_config: '{}', provider_config: {}, is_active: false,
      api_token: '', has_token: false,
    });
    setError(null);
    setFormOpen(true);
  };

  const openEdit = async (provider) => {
    setError(null);
    try {
      const res = await api.get(`/dashboard/providers/${provider.id}/`);
      const d = res.data;
      let extraConfig = '{}';
      try { extraConfig = JSON.stringify(d.extra_config || {}, null, 2); } catch { /* keep {} */ }
      setForm({
        id: d.id,
        name: d.name || '',
        slug: d.slug || '',
        adapter_key: d.adapter_key || '',
        api_endpoint: d.api_endpoint || '',
        extra_config: extraConfig,
        provider_config: (d.provider_config && typeof d.provider_config === 'object') ? d.provider_config : {},
        is_active: !!d.is_active,
        api_token: '',
        has_token: !!d.has_token,
      });
      setFormOpen(true);
    } catch {
      addToast('Failed to load provider.', 'error');
    }
  };

  const displayFields = () => {
    try {
      const parsed = JSON.parse(form.extra_config || '{}');
      const fields = parsed && parsed.display && parsed.display.fields;
      return Array.isArray(fields) ? fields : [];
    } catch {
      return [];
    }
  };

  const setFieldValue = (key, value) => {
    setForm(prev => ({ ...prev, provider_config: { ...prev.provider_config, [key]: value } }));
  };

  const handleSave = async (e) => {
    e.preventDefault();
    setSaving(true);
    setError(null);
    let extraConfig;
    try {
      extraConfig = JSON.parse(form.extra_config || '{}');
    } catch {
      setError('Extra config must be valid JSON.');
      setSaving(false);
      return;
    }
    const body = {
      name: form.name,
      slug: form.slug || slugify(form.name),
      adapter_key: form.adapter_key,
      api_endpoint: form.api_endpoint,
      extra_config: extraConfig,
      provider_config: form.provider_config,
      is_active: form.is_active,
      api_token: form.api_token,
    };
    try {
      if (form.id) {
        await api.put(`/dashboard/providers/${form.id}/`, body);
        addToast('Provider updated!', 'success');
      } else {
        await api.post('/dashboard/providers/', body);
        addToast('Provider created!', 'success');
      }
      setFormOpen(false);
      fetchProviders();
    } catch (err) {
      const detail = err?.response?.data?.detail || err?.response?.data?.error || 'Failed to save provider.';
      setError(typeof detail === 'string' ? detail : 'Failed to save provider.');
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (provider) => {
    if (!window.confirm(`Delete provider "${provider.name}"?`)) return;
    try {
      await api.delete(`/dashboard/providers/${provider.id}/`);
      addToast('Provider deleted.', 'success');
      fetchProviders();
    } catch (err) {
      const detail = err?.response?.data?.detail || err?.response?.data?.error || 'Failed to delete provider.';
      addToast(typeof detail === 'string' ? detail : 'Failed to delete provider.', 'error');
    }
  };

  if (loading) {
    return (
      <AdminLayout>
        <div className="admin-content">
          <div className="h-7 w-32 bg-gray-200 animate-pulse rounded mb-2" />
          <div className="h-4 w-64 bg-gray-200 animate-pulse rounded mb-6" />
          <div className="admin-card">
            <div className="admin-card-body space-y-4">
              <div className="h-5 w-48 bg-gray-200 animate-pulse rounded" />
              <div className="h-4 w-full bg-gray-200 animate-pulse rounded" />
              <div className="h-4 w-3/4 bg-gray-200 animate-pulse rounded" />
            </div>
          </div>
        </div>
      </AdminLayout>
    );
  }

  if (formOpen) {
    const fields = displayFields();
    return (
      <AdminLayout>
        <h1 style={{ fontSize: 24, fontWeight: 700, color: '#1e293b', marginBottom: 8 }}>
          {form.id ? '✏️ Edit Provider' : '➕ Add Provider'}
        </h1>
        <p style={{ fontSize: 14, color: '#64748b', marginBottom: 24 }}>
          {form.id ? 'Update provider configuration' : 'Create a new provider connection'}
        </p>

        <div className="admin-card" style={{ maxWidth: 640 }}>
          <div className="admin-card-body">
            <form onSubmit={handleSave}>
              <div className="admin-field">
                <label className="admin-label">Name</label>
                <input
                  type="text"
                  className="admin-input"
                  value={form.name}
                  onChange={e => {
                    setForm(prev => ({
                      ...prev,
                      name: e.target.value,
                      slug: form.id ? prev.slug : slugify(e.target.value),
                    }));
                  }}
                  required
                />
              </div>

              <div className="admin-field">
                <label className="admin-label">Slug</label>
                <input
                  type="text"
                  className="admin-input"
                  value={form.slug}
                  onChange={e => setForm(prev => ({ ...prev, slug: e.target.value }))}
                  placeholder="auto-generated from name"
                />
              </div>

              <div className="admin-field">
                <label className="admin-label">Adapter key</label>
                <input
                  type="text"
                  className="admin-input"
                  value={form.adapter_key}
                  onChange={e => setForm(prev => ({ ...prev, adapter_key: e.target.value }))}
                  required
                />
                <p style={{ fontSize: 12, color: '#94a3b8', marginTop: 4 }}>
                  Determines which adapter class handles API calls for this provider.
                </p>
              </div>

              <div className="admin-field">
                <label className="admin-label">Api endpoint</label>
                {form.api_endpoint ? (
                  <p style={{ fontSize: 13, color: '#475569', marginBottom: 6, wordBreak: 'break-all' }}>
                    Currently: <code style={{ color: '#0f172a' }}>{form.api_endpoint}</code>
                  </p>
                ) : null}
                <input
                  type="text"
                  className="admin-input"
                  value={form.api_endpoint}
                  onChange={e => setForm(prev => ({ ...prev, api_endpoint: e.target.value }))}
                  placeholder="Change: https://panel.example.com/api.php"
                />
              </div>

              <div className="admin-field">
                <label className="admin-label">Extra config</label>
                <textarea
                  className="admin-input"
                  rows={5}
                  value={form.extra_config}
                  onChange={e => setForm(prev => ({ ...prev, extra_config: e.target.value }))}
                  style={{ fontFamily: 'monospace', fontSize: 12, width: '100%', boxSizing: 'border-box' }}
                />
                <p style={{ fontSize: 12, color: '#94a3b8', marginTop: 4 }}>
                  Provider-specific config (e.g. {'{"dns_domain": "kmapp.xyz", "port": 8080}'})
                </p>
              </div>

              <div className="admin-field" style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <input
                  type="checkbox"
                  checked={form.is_active}
                  onChange={e => setForm(prev => ({ ...prev, is_active: e.target.checked }))}
                  id="provider-is-active"
                />
                <label className="admin-label" style={{ marginBottom: 0, cursor: 'pointer' }} htmlFor="provider-is-active">
                  Is active
                </label>
              </div>

              <div className="admin-field">
                <label className="admin-label">API Token</label>
                <input
                  type="password"
                  className="admin-input"
                  value={form.api_token}
                  onChange={e => setForm(prev => ({ ...prev, api_token: e.target.value }))}
                  placeholder="••••"
                  autoComplete="new-password"
                />
                <p style={{ fontSize: 12, color: '#94a3b8', marginTop: 4 }}>
                  Enter a new token to update. Leave blank to keep existing.
                </p>
              </div>

              {fields.length > 0 && (
                <>
                  <h2 style={{ fontSize: 15, fontWeight: 600, color: '#1e293b', margin: '20px 0 12px' }}>
                    Dynamic Fields
                  </h2>
                  {fields.map((field, i) => (
                    <div className="admin-field" key={i}>
                      <label className="admin-label">{field.label || field.name}</label>
                      <FieldInput
                        field={field}
                        value={form.provider_config[field.name]}
                        onChange={v => setFieldValue(field.name, v)}
                      />
                    </div>
                  ))}
                </>
              )}

              {error && (
                <div className="admin-error" style={{ color: '#dc2626', fontSize: 13, margin: '12px 0' }}>
                  {error}
                </div>
              )}

              <div style={{ display: 'flex', gap: 8, marginTop: 16 }}>
                <button type="submit" className="admin-btn admin-btn-primary" disabled={saving} style={{ padding: '10px 24px' }}>
                  {saving ? 'Saving...' : 'SAVE'}
                </button>
                {form.id && (
                  <button
                    type="button"
                    className="admin-btn admin-btn-danger"
                    onClick={() => handleDelete({ id: form.id, name: form.name })}
                    disabled={saving}
                  >
                    Delete
                  </button>
                )}
                <button type="button" className="admin-btn" onClick={() => setFormOpen(false)} disabled={saving}>
                  Cancel
                </button>
              </div>
            </form>
          </div>
        </div>
      </AdminLayout>
    );
  }

  return (
    <AdminLayout>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
        <h1 style={{ fontSize: 24, fontWeight: 700, color: '#1e293b' }}>📡 Providers</h1>
        <button type="button" className="admin-btn admin-btn-primary" onClick={openCreate} style={{ padding: '10px 20px' }}>
          + Add Provider
        </button>
      </div>
      <p style={{ fontSize: 14, color: '#64748b', marginBottom: 24 }}>Manage API provider connections</p>

      <div className="admin-card">
        <div className="admin-card-body">
          <input
            type="text"
            className="admin-input"
            placeholder="Search by name, slug or adapter key..."
            value={search}
            onChange={e => { setSearch(e.target.value); setPage(1); }}
            style={{ marginBottom: 16, maxWidth: 400 }}
          />
          <div style={{ overflowX: 'auto' }}>
            <table className="admin-table">
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Slug</th>
                  <th>Adapter key</th>
                  <th>Api endpoint</th>
                  <th>Active</th>
                  <th>Token</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {pageRows.length === 0 ? (
                  <tr>
                    <td colSpan="7" style={{ textAlign: 'center', color: '#94a3b8', padding: 16 }}>
                      No providers found.
                    </td>
                  </tr>
                ) : (
                  pageRows.map(p => (
                    <tr key={p.id}>
                      <td style={{ fontWeight: 600, color: '#1e293b' }}>{p.name}</td>
                      <td style={{ color: '#475569' }}>{p.slug}</td>
                      <td><code style={{ fontSize: 12, color: '#0f172a' }}>{p.adapter_key}</code></td>
                      <td style={{ fontSize: 12, color: '#64748b', wordBreak: 'break-all', maxWidth: 260 }}>{p.api_endpoint || '—'}</td>
                      <td>
                        <span style={{
                          fontSize: 11, fontWeight: 600, padding: '2px 8px', borderRadius: 999,
                          background: p.is_active ? '#dcfce7' : '#f1f5f9',
                          color: p.is_active ? '#16a34a' : '#64748b',
                        }}>
                          {p.is_active ? 'Active' : 'Inactive'}
                        </span>
                      </td>
                      <td>
                        <span style={{
                          fontSize: 11, fontWeight: 600, padding: '2px 8px', borderRadius: 999,
                          background: p.has_token ? '#fef9c3' : '#f1f5f9',
                          color: p.has_token ? '#a16207' : '#94a3b8',
                        }}>
                          {p.has_token ? 'Set' : 'None'}
                        </span>
                      </td>
                      <td style={{ whiteSpace: 'nowrap' }}>
                        <button type="button" className="admin-btn admin-btn-sm" onClick={() => openEdit(p)}>
                          Edit
                        </button>{' '}
                        <button type="button" className="admin-btn admin-btn-sm admin-btn-danger" onClick={() => handleDelete(p)}>
                          Delete
                        </button>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
          {totalPages > 1 && (
            <div className="admin-pagination" style={{ marginTop: 16 }}>
              <button type="button" className="admin-btn admin-btn-sm" disabled={page <= 1} onClick={() => setPage(page - 1)}>
                Prev
              </button>
              <span style={{ margin: '0 12px', fontSize: 13, color: '#64748b' }}>Page {page} of {totalPages}</span>
              <button type="button" className="admin-btn admin-btn-sm" disabled={page >= totalPages} onClick={() => setPage(page + 1)}>
                Next
              </button>
            </div>
          )}
        </div>
      </div>
    </AdminLayout>
  );
}
