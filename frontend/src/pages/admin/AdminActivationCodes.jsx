import { useState, useEffect, useCallback } from 'react';
import { Link } from 'react-router-dom';
import AdminLayout from '../../components/AdminLayout';
import api from '../../lib/axios';

export default function AdminActivationCodes() {
  const [credentials, setCredentials] = useState([]);
  const [products, setProducts] = useState([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState('');
  const [productFilter, setProductFilter] = useState('');
  const [stats, setStats] = useState({ total: 0, available: 0, used: 0 });

  useEffect(() => {
    // Fetch all manual products for the filter dropdown
    api.get('/dashboard/manual-products/')
      .then(res => setProducts(res.data.results || res.data))
      .catch(console.error);
  }, []);

  const fetchCredentials = useCallback(async () => {
    setLoading(true);
    try {
      // If a product filter is selected, fetch credentials for that product
      if (productFilter) {
        const params = new URLSearchParams();
        if (statusFilter) params.set('status', statusFilter);
        if (search) params.set('search', search);
        const { data } = await api.get(`/dashboard/manual-products/${productFilter}/?${params}`);
        setCredentials(data.results || []);
        setStats(data.stats || { total: 0, available: 0, used: 0 });
      } else {
        // Show aggregate stats from all products
        const allCreds = [];
        let totalStats = { total: 0, available: 0, used: 0 };
        for (const p of products) {
          try {
            const params = new URLSearchParams();
            if (statusFilter) params.set('status', statusFilter);
            if (search) params.set('search', search);
            params.set('page_size', '100');
            const { data } = await api.get(`/dashboard/manual-products/${p.id}/?${params}`);
            allCreds.push(...(data.results || []));
            if (data.stats) {
              totalStats.total += data.stats.total || 0;
              totalStats.available += data.stats.available || 0;
              totalStats.used += data.stats.used || 0;
            }
          } catch (err) {
            // skip errored product
          }
        }
        setCredentials(allCreds);
        setStats(totalStats);
      }
    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  }, [productFilter, statusFilter, search, products]);

  useEffect(() => {
    if (products.length > 0 || productFilter) {
      fetchCredentials();
    } else {
      setLoading(false);
    }
  }, [fetchCredentials, products.length, productFilter]);

  const handleDelete = async (credId) => {
    if (!window.confirm('Delete this credential?')) return;
    try {
      await api.delete(`/dashboard/credentials/${credId}/`);
      fetchCredentials();
    } catch (err) {
      console.error(err);
    }
  };

  return (
    <AdminLayout>
      <div style={{ marginBottom: 24 }}>
        <h1 style={{ fontSize: 24, fontWeight: 700, color: '#1e293b' }}>🔑 Activation Codes</h1>
        <p style={{ fontSize: 14, color: '#64748b' }}>Browse all manual product credentials</p>
      </div>

      {/* Stats */}
      <div className="admin-stats-grid" style={{ marginBottom: 20 }}>
        <div className="admin-stat-card purple">
          <div className="admin-stat-label">Total</div>
          <div className="admin-stat-value">{stats.total}</div>
          <div className="admin-stat-icon">🔑</div>
        </div>
        <div className="admin-stat-card green">
          <div className="admin-stat-label">Available</div>
          <div className="admin-stat-value">{stats.available}</div>
          <div className="admin-stat-icon">✅</div>
        </div>
        <div className="admin-stat-card amber">
          <div className="admin-stat-label">Used</div>
          <div className="admin-stat-value">{stats.used}</div>
          <div className="admin-stat-icon">📦</div>
        </div>
      </div>

      {/* Filters */}
      <div className="admin-card" style={{ marginBottom: 20 }}>
        <div className="admin-card-body" style={{ padding: 16 }}>
          <div className="admin-search">
            <input
              type="text"
              className="admin-search-input"
              placeholder="Search credentials..."
              value={search}
              onChange={e => setSearch(e.target.value)}
            />
            <select
              className="admin-select"
              value={productFilter}
              onChange={e => setProductFilter(e.target.value)}
            >
              <option value="">All Products</option>
              {products.map(p => (
                <option key={p.id} value={p.id}>{p.name}</option>
              ))}
            </select>
            <select
              className="admin-select"
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
          <div className="admin-loading">
            <div className="admin-spinner"></div>
            Loading codes...
          </div>
        ) : (
          <div className="admin-table-wrap">
            <table className="admin-table">
              <thead>
                <tr>
                  <th>Credential</th>
                  <th>Product</th>
                  <th>Type</th>
                  <th>Status</th>
                  <th>Assigned To</th>
                  <th>Created</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {credentials.length === 0 ? (
                  <tr>
                    <td colSpan="7">
                      <div className="admin-empty">
                        <div className="admin-empty-icon">🔑</div>
                        <div className="admin-empty-text">No activation codes found</div>
                      </div>
                    </td>
                  </tr>
                ) : (
                  credentials.map(c => (
                    <tr key={c.id}>
                      <td data-label="Credential" style={{ fontFamily: 'monospace', fontWeight: 500 }}>
                        {c.credential_type === 'username_password'
                          ? c.username
                          : (c.code?.length > 25 ? c.code.slice(0, 25) + '...' : c.code)
                        }
                      </td>
                      <td data-label="Product">
                        <Link
                          to={`/admin/products/${c.product}`}
                          style={{ color: '#6366f1', textDecoration: 'none', fontWeight: 500 }}
                        >
                          {c.product_name}
                        </Link>
                      </td>
                      <td data-label="Type">
                        <span className={`admin-badge ${c.credential_type === 'username_password' ? 'blue' : 'amber'}`}>
                          {c.credential_type === 'username_password' ? 'U:P' : 'Code'}
                        </span>
                      </td>
                      <td data-label="Status">
                        <span className={`admin-badge ${
                          c.status === 'available' ? 'green' :
                          c.status === 'used' ? 'gray' : 'red'
                        }`}>
                          {c.status}
                        </span>
                      </td>
                      <td data-label="Assigned To">{c.assigned_to_username || '—'}</td>
                      <td data-label="Created" style={{ fontSize: 13, color: '#64748b' }}>
                        {new Date(c.created_at).toLocaleDateString()}
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
        )}
      </div>
    </AdminLayout>
  );
}
