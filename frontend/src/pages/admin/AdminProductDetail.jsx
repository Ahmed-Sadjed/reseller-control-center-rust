import { useState, useEffect, useCallback } from 'react';
import { useParams, Link } from 'react-router-dom';
import AdminLayout from '../../components/AdminLayout';
import TableSkeleton from '../../components/skeletons/TableSkeleton';
import { useToast } from '../../context/ToastContext';
import api from '../../lib/axios';

export default function AdminProductDetail() {
  const { id } = useParams();
  const [product, setProduct] = useState(null);
  const [credentials, setCredentials] = useState([]);
  const [variants, setVariants] = useState([]);
  const [stats, setStats] = useState({});
  const [loading, setLoading] = useState(true);
  const [statusFilter, setStatusFilter] = useState('');
  const [search, setSearch] = useState('');
  const [page, setPage] = useState(1);
  const [pagination, setPagination] = useState({ count: 0, next: null, previous: null });

  // Add modal
  const [showAddModal, setShowAddModal] = useState(false);
  const [addForm, setAddForm] = useState({ variant_id: '', username: '', password: '', code: '', notes: '', expires_at: '' });
  const [addLoading, setAddLoading] = useState(false);
  const [addError, setAddError] = useState('');
  const { addToast } = useToast();

  const fetchData = useCallback(async () => {
    setLoading(true);
    try {
      const params = new URLSearchParams();
      params.set('page', page);
      if (statusFilter) params.set('status', statusFilter);
      if (search) params.set('search', search);
      const { data } = await api.get(`/dashboard/manual-products/${id}/?${params}`);
      setProduct(data.product);
      setVariants(data.product?.variants || []);
      setCredentials(data.results || []);
      setStats(data.stats || {});
      setPagination({ count: data.count || 0, next: data.next, previous: data.previous });
    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  }, [id, page, statusFilter, search]);

  useEffect(() => { fetchData(); }, [fetchData]);
  useEffect(() => { setPage(1); }, [statusFilter, search]);

  const showAlertMsg = (msg, type = 'success') => {
    addToast(msg, type);
  };

  const handleAdd = async (e) => {
    e.preventDefault();
    setAddLoading(true);
    setAddError('');
    try {
      const payload = {
        variant_id: addForm.variant_id ? Number(addForm.variant_id) : null,
        username: addForm.username,
        password: addForm.password,
        code: addForm.code,
        notes: addForm.notes,
        expires_at: addForm.expires_at || null,
      };
      await api.post(`/dashboard/manual-products/${id}/credentials/`, payload);
      setShowAddModal(false);
      setAddForm({ variant_id: '', username: '', password: '', code: '', notes: '', expires_at: '' });
      showAlertMsg('Credential added!');
      fetchData();
    } catch (err) {
      const data = err.response?.data;
      setAddError(typeof data === 'string' ? data : JSON.stringify(data));
    } finally {
      setAddLoading(false);
    }
  };

  const handleDelete = async (credId) => {
    if (!window.confirm('Delete this credential?')) return;
    try {
      await api.delete(`/dashboard/credentials/${credId}/`);
      showAlertMsg('Credential deleted.');
      fetchData();
    } catch (err) {
      showAlertMsg('Failed to delete.', 'error');
    }
  };

  if (loading && !product) {
    return (
      <AdminLayout>
        <div className="admin-card" style={{ margin: '28px' }}>
          <TableSkeleton rows={5} cols={7} columnWidths={['80px', '120px', '120px', '70px', '100px', '90px', '60px']} />
        </div>
      </AdminLayout>
    );
  }

  const isUP = product?.credential_type === 'username_password';

  return (
    <AdminLayout>
      <Link to="/admin/products" style={{ fontSize: 14, color: '#6366f1', textDecoration: 'none', marginBottom: 16, display: 'inline-block' }}>
        ← Back to Products
      </Link>

      {/* Product Info */}
      <div className="admin-card" style={{ marginBottom: 24 }}>
        <div className="admin-card-body">
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', flexWrap: 'wrap', gap: 16 }}>
            <div>
              <h1 style={{ fontSize: 22, fontWeight: 700, color: '#1e293b', marginBottom: 8 }}>
                📦 {product?.name}
              </h1>
              <div style={{ display: 'flex', gap: 12, flexWrap: 'wrap' }}>
                <span className={`admin-badge ${isUP ? 'blue' : 'amber'}`}>
                  {isUP ? 'Username + Password' : 'Single Code'}
                </span>
                <span className={`admin-badge ${product?.is_active ? 'green' : 'red'}`}>
                  {product?.is_active ? '● Active' : '● Inactive'}
                </span>
              </div>
            </div>
            <button className="admin-btn admin-btn-primary" onClick={() => setShowAddModal(true)}>
              ➕ Add Credential
            </button>
          </div>
        </div>
      </div>

      {/* Stats */}
      <div className="admin-stats-grid" style={{ marginBottom: 20 }}>
        <div className="admin-stat-card purple">
          <div className="admin-stat-label">Total</div>
          <div className="admin-stat-value">{stats.total || 0}</div>
        </div>
        <div className="admin-stat-card green">
          <div className="admin-stat-label">Available</div>
          <div className="admin-stat-value">{stats.available || 0}</div>
        </div>
        <div className="admin-stat-card amber">
          <div className="admin-stat-label">Used</div>
          <div className="admin-stat-value">{stats.used || 0}</div>
        </div>
      </div>

      {/* Filters */}
      <div className="admin-card" style={{ marginBottom: 20 }}>
        <div className="admin-card-body" style={{ padding: 16 }}>
          <div className="admin-search">
            <input
              type="text"
              className="admin-search-input"
              placeholder={isUP ? 'Search username...' : 'Search code...'}
              value={search}
              onChange={e => setSearch(e.target.value)}
            />
            <select
              className="admin-select"
              style={{ width: 160 }}
              value={statusFilter}
              onChange={e => setStatusFilter(e.target.value)}
            >
              <option value="">All Status</option>
              <option value="available">Available</option>
              <option value="used">Used</option>
              <option value="expired">Expired</option>
            </select>
          </div>
        </div>
      </div>

      {/* Credentials Table */}
      <div className="admin-card">
        {loading ? (
          <TableSkeleton rows={5} cols={7} columnWidths={['80px', '120px', '120px', '70px', '100px', '90px', '60px']} />
        ) : (
          <>
            <div className="admin-table-wrap">
              <table className="admin-table">
                <thead>
                  <tr>
                    <th>Duration</th>
                    {isUP ? (
                      <>
                        <th>Username</th>
                        <th>Password</th>
                      </>
                    ) : (
                      <th>Code</th>
                    )}
                    <th>Status</th>
                    <th>Assigned To</th>
                    <th>Notes</th>
                    <th>Expires</th>
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {credentials.length === 0 ? (
                    <tr>
                      <td colSpan={isUP ? 8 : 7}>
                        <div className="admin-empty">
                          <div className="admin-empty-icon">🔑</div>
                          <div className="admin-empty-text">No credentials found</div>
                        </div>
                      </td>
                    </tr>
                  ) : (
                    credentials.map(c => (
                      <tr key={c.id}>
                        <td data-label="Duration" style={{ fontSize: 13, color: '#64748b' }}>
                          {c.variant_display || '—'}
                        </td>
                        {isUP ? (
                          <>
                            <td data-label="Username" style={{ fontFamily: 'monospace', fontWeight: 500 }}>{c.username}</td>
                            <td data-label="Password" style={{ fontFamily: 'monospace' }}>{'●'.repeat(8)}</td>
                          </>
                        ) : (
                          <td data-label="Code" style={{ fontFamily: 'monospace', fontWeight: 500 }}>
                            {c.code?.length > 30 ? c.code.slice(0, 30) + '...' : c.code}
                          </td>
                        )}
                        <td data-label="Status">
                          <span className={`admin-badge ${
                            c.status === 'available' ? 'green' :
                            c.status === 'used' ? 'gray' : 'red'
                          }`}>
                            {c.status}
                          </span>
                        </td>
                        <td data-label="Assigned To">{c.assigned_to_username || '—'}</td>
                        <td data-label="Notes" style={{ maxWidth: 150, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', color: '#64748b' }}>
                          {c.notes || '—'}
                        </td>
                        <td data-label="Expires" style={{ fontSize: 13, color: '#64748b' }}>
                          {c.expires_at ? new Date(c.expires_at).toLocaleDateString() : '—'}
                        </td>
                        <td data-label="Actions">
                          <button className="admin-btn admin-btn-danger admin-btn-sm" onClick={() => handleDelete(c.id)}>
                            🗑️
                          </button>
                        </td>
                      </tr>
                    ))
                  )}
                </tbody>
              </table>
            </div>

            {pagination.count > 0 && (
              <div className="admin-pagination">
                <button className="admin-page-btn" disabled={!pagination.previous} onClick={() => setPage(p => Math.max(1, p - 1))}>
                  ← Prev
                </button>
                <span className="admin-page-info">Page {page} · {pagination.count} total</span>
                <button className="admin-page-btn" disabled={!pagination.next} onClick={() => setPage(p => p + 1)}>
                  Next →
                </button>
              </div>
            )}
          </>
        )}
      </div>

      {/* Add Credential Modal */}
      {showAddModal && (
        <div className="admin-modal-overlay" onClick={() => setShowAddModal(false)}>
          <div className="admin-modal" onClick={e => e.stopPropagation()}>
            <div className="admin-modal-header">
              <div className="admin-modal-title">➕ Add Credential</div>
              <button className="admin-modal-close" onClick={() => setShowAddModal(false)}>✕</button>
            </div>
            <form onSubmit={handleAdd}>
              <div className="admin-modal-body">
                {addError && <div className="admin-alert error">⚠️ {addError}</div>}

                <p style={{ fontSize: 13, color: '#64748b', marginBottom: 16 }}>
                  Product: <strong>{product?.name}</strong> · Type: <strong>{isUP ? 'Username + Password' : 'Single Code'}</strong>
                </p>

                <div className="admin-field">
                  <label className="admin-label">Duration *</label>
                  <select
                    className="admin-select"
                    style={{ width: '100%' }}
                    required
                    value={addForm.variant_id}
                    onChange={e => setAddForm(f => ({ ...f, variant_id: e.target.value }))}
                  >
                    <option value="">Select duration...</option>
                    {variants.map(v => (
                      <option key={v.id} value={v.id}>
                        {v.is_lifetime ? 'Lifetime' : (() => {
                          const labels = {
                            100: '6 Hours', 101: '12 Hours', 102: '24 Hours', 103: '72 Hours',
                            1: '1 Month', 3: '3 Months', 6: '6 Months', 12: '12 Months',
                            15: '15 Months', 24: '2 Years', 36: '3 Years',
                          };
                          return labels[v.duration_months] || `${v.duration_months} Months`;
                        })()} — {v.price_in_credits} credits
                      </option>
                    ))}
                  </select>
                </div>

                {isUP ? (
                  <>
                    <div className="admin-field">
                      <label className="admin-label">Username *</label>
                      <input
                        type="text"
                        className="admin-input"
                        required
                        value={addForm.username}
                        onChange={e => setAddForm(f => ({ ...f, username: e.target.value }))}
                      />
                    </div>
                    <div className="admin-field">
                      <label className="admin-label">Password *</label>
                      <input
                        type="text"
                        className="admin-input"
                        required
                        value={addForm.password}
                        onChange={e => setAddForm(f => ({ ...f, password: e.target.value }))}
                      />
                    </div>
                  </>
                ) : (
                  <div className="admin-field">
                    <label className="admin-label">Activation Code *</label>
                    <input
                      type="text"
                      className="admin-input"
                      required
                      value={addForm.code}
                      onChange={e => setAddForm(f => ({ ...f, code: e.target.value }))}
                    />
                  </div>
                )}

                <div className="admin-field">
                  <label className="admin-label">Notes</label>
                  <input
                    type="text"
                    className="admin-input"
                    value={addForm.notes}
                    onChange={e => setAddForm(f => ({ ...f, notes: e.target.value }))}
                    placeholder="Optional"
                  />
                </div>
                <div className="admin-field">
                  <label className="admin-label">Expires At</label>
                  <input
                    type="datetime-local"
                    className="admin-input"
                    value={addForm.expires_at}
                    onChange={e => setAddForm(f => ({ ...f, expires_at: e.target.value }))}
                  />
                </div>
              </div>
              <div className="admin-modal-footer">
                <button type="button" className="admin-btn admin-btn-secondary" onClick={() => setShowAddModal(false)}>
                  Cancel
                </button>
                <button type="submit" className="admin-btn admin-btn-primary" disabled={addLoading}>
                  {addLoading ? 'Adding...' : '➕ Add Credential'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </AdminLayout>
  );
}
