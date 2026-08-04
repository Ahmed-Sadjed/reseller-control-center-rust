import { useState, useEffect, useCallback } from 'react';
import { Link } from 'react-router-dom';
import AdminLayout from '../../components/AdminLayout';
import TableSkeleton from '../../components/skeletons/TableSkeleton';
import api from '../../lib/axios';

function formatCredits(n) {
  if (n == null) return '0.00';
  return Number(n).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

export default function AdminResellers() {
  const [resellers, setResellers] = useState([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState('');
  const [page, setPage] = useState(1);
  const [pagination, setPagination] = useState({ count: 0, next: null, previous: null });
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [creating, setCreating] = useState(false);
  const [formError, setFormError] = useState('');
  const [form, setForm] = useState({ username: '', password: '', password_confirm: '', initial_credits: '0' });

  const fetchResellers = useCallback(async () => {
    setLoading(true);
    try {
      const params = new URLSearchParams();
      params.set('page', page);
      if (search) params.set('search', search);
      if (statusFilter) params.set('status', statusFilter);
      const { data } = await api.get(`/dashboard/resellers/?${params}`);
      setResellers(data.results || data);
      setPagination({ count: data.count || 0, next: data.next, previous: data.previous });
    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  }, [page, search, statusFilter]);

  useEffect(() => {
    fetchResellers();
  }, [fetchResellers]);

  // Debounced search
  useEffect(() => {
    setPage(1);
  }, [search, statusFilter]);

  const handleCreate = async (e) => {
    e.preventDefault();
    setCreating(true);
    setFormError('');
    try {
      await api.post('/dashboard/resellers/', {
        username: form.username,
        password: form.password,
        password_confirm: form.password_confirm,
        initial_credits: parseFloat(form.initial_credits) || 0,
      });
      setShowCreateModal(false);
      setForm({ username: '', password: '', password_confirm: '', initial_credits: '0' });
      fetchResellers();
    } catch (err) {
      const data = err.response?.data;
      if (data) {
        const msg = typeof data === 'string' ? data :
          Object.values(data).flat().join('. ');
        setFormError(msg);
      } else {
        setFormError('Failed to create reseller.');
      }
    } finally {
      setCreating(false);
    }
  };

  const handleToggle = async (id) => {
    try {
      await api.post(`/dashboard/resellers/${id}/toggle/`);
      fetchResellers();
    } catch (err) {
      console.error(err);
    }
  };

  return (
    <AdminLayout>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 24 }}>
        <div>
          <h1 style={{ fontSize: 24, fontWeight: 700, color: '#1e293b' }}>👥 Resellers</h1>
          <p style={{ fontSize: 14, color: '#64748b' }}>Manage reseller accounts and credits</p>
        </div>
        <button className="admin-btn admin-btn-primary" onClick={() => setShowCreateModal(true)}>
          ➕ Add Reseller
        </button>
      </div>

      {/* Search / Filter */}
      <div className="admin-card" style={{ marginBottom: 20 }}>
        <div className="admin-card-body" style={{ padding: 16 }}>
          <div className="admin-search">
            <input
              type="text"
              className="admin-search-input"
              placeholder="Search by username..."
              value={search}
              onChange={e => setSearch(e.target.value)}
            />
            <select
              className="admin-select"
              value={statusFilter}
              onChange={e => setStatusFilter(e.target.value)}
            >
              <option value="">All Status</option>
              <option value="active">Active</option>
              <option value="inactive">Inactive</option>
            </select>
          </div>
        </div>
      </div>

      {/* Reseller Table */}
      <div className="admin-card">
        {loading ? (
          <TableSkeleton rows={5} cols={7} columnWidths={['120px', '80px', '60px', '80px', '70px', '90px', '120px']} />
        ) : (
          <>
            <div className="admin-table-wrap">
              <table className="admin-table">
                <thead>
                  <tr>
                    <th>Username</th>
                    <th>Credits</th>
                    <th>Orders</th>
                    <th>Revenue</th>
                    <th>Status</th>
                    <th>Joined</th>
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {resellers.length === 0 ? (
                    <tr>
                      <td colSpan="7">
                        <div className="admin-empty">
                          <div className="admin-empty-icon">👥</div>
                          <div className="admin-empty-text">No resellers found</div>
                        </div>
                      </td>
                    </tr>
                  ) : (
                    resellers.map(r => (
                      <tr key={r.id}>
                        <td data-label="Username">
                          <Link to={`/admin/resellers/${r.id}`} style={{ color: '#1e293b', fontWeight: 600, textDecoration: 'none' }}>
                            {r.username}
                          </Link>
                        </td>
                        <td data-label="Credits" style={{ fontWeight: 500 }}>{formatCredits(r.credit_balance)}</td>
                        <td data-label="Orders">{r.order_count || 0}</td>
                        <td data-label="Revenue" style={{ fontWeight: 500 }}>{formatCredits(r.total_revenue)}</td>
                        <td data-label="Status">
                          <span className={`admin-badge ${r.is_active ? 'green' : 'red'}`}>
                            {r.is_active ? '● Active' : '● Inactive'}
                          </span>
                        </td>
                        <td data-label="Joined" style={{ color: '#64748b', fontSize: 13 }}>
                          {new Date(r.date_joined).toLocaleDateString()}
                        </td>
                        <td data-label="Actions">
                          <div style={{ display: 'flex', gap: 6 }}>
                            <Link to={`/admin/resellers/${r.id}`} className="admin-btn admin-btn-secondary admin-btn-sm">
                              View
                            </Link>
                            <button
                              className={`admin-btn admin-btn-sm ${r.is_active ? 'admin-btn-danger' : 'admin-btn-success'}`}
                              onClick={() => handleToggle(r.id)}
                            >
                              {r.is_active ? 'Disable' : 'Enable'}
                            </button>
                          </div>
                        </td>
                      </tr>
                    ))
                  )}
                </tbody>
              </table>
            </div>

            {/* Pagination */}
            {pagination.count > 0 && (
              <div className="admin-pagination">
                <button
                  className="admin-page-btn"
                  disabled={!pagination.previous}
                  onClick={() => setPage(p => Math.max(1, p - 1))}
                >
                  ← Prev
                </button>
                <span className="admin-page-info">
                  Page {page} · {pagination.count} total
                </span>
                <button
                  className="admin-page-btn"
                  disabled={!pagination.next}
                  onClick={() => setPage(p => p + 1)}
                >
                  Next →
                </button>
              </div>
            )}
          </>
        )}
      </div>

      {/* Create Modal */}
      {showCreateModal && (
        <div className="admin-modal-overlay" onClick={() => setShowCreateModal(false)}>
          <div className="admin-modal" onClick={e => e.stopPropagation()}>
            <div className="admin-modal-header">
              <div className="admin-modal-title">➕ Add New Reseller</div>
              <button className="admin-modal-close" onClick={() => setShowCreateModal(false)}>✕</button>
            </div>
            <form onSubmit={handleCreate}>
              <div className="admin-modal-body">
                {formError && (
                  <div className="admin-alert error">⚠️ {formError}</div>
                )}
                <div className="admin-field">
                  <label className="admin-label">Username *</label>
                  <input
                    type="text"
                    className="admin-input"
                    required
                    value={form.username}
                    onChange={e => setForm(f => ({ ...f, username: e.target.value }))}
                    placeholder="Enter username"
                  />
                </div>
                <div className="admin-field">
                  <label className="admin-label">Password *</label>
                  <input
                    type="password"
                    className="admin-input"
                    required
                    minLength={6}
                    value={form.password}
                    onChange={e => setForm(f => ({ ...f, password: e.target.value }))}
                    placeholder="Min 6 characters"
                  />
                </div>
                <div className="admin-field">
                  <label className="admin-label">Confirm Password *</label>
                  <input
                    type="password"
                    className="admin-input"
                    required
                    minLength={6}
                    value={form.password_confirm}
                    onChange={e => setForm(f => ({ ...f, password_confirm: e.target.value }))}
                    placeholder="Confirm password"
                  />
                </div>
                <div className="admin-field">
                  <label className="admin-label">Initial Credits</label>
                  <input
                    type="number"
                    step="0.01"
                    min="0"
                    className="admin-input"
                    value={form.initial_credits}
                    onChange={e => setForm(f => ({ ...f, initial_credits: e.target.value }))}
                  />
                </div>
              </div>
              <div className="admin-modal-footer">
                <button type="button" className="admin-btn admin-btn-secondary" onClick={() => setShowCreateModal(false)}>
                  Cancel
                </button>
                <button type="submit" className="admin-btn admin-btn-primary" disabled={creating}>
                  {creating ? 'Creating...' : '➕ Create Reseller'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </AdminLayout>
  );
}
