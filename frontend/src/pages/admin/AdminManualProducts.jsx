import { useState, useEffect } from 'react';
import { Link } from 'react-router-dom';
import AdminLayout from '../../components/AdminLayout';
import api from '../../lib/axios';

export default function AdminManualProducts() {
  const [products, setProducts] = useState([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');

  useEffect(() => {
    const params = new URLSearchParams();
    if (search) params.set('search', search);
    api.get(`/dashboard/manual-products/?${params}`)
      .then(res => setProducts(res.data.results || res.data))
      .catch(console.error)
      .finally(() => setLoading(false));
  }, [search]);

  const typeLabel = (t) => {
    if (t === 'username_password') return 'U:P Pair';
    if (t === 'single_code') return 'Single Code';
    return t || '—';
  };

  return (
    <AdminLayout>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 24 }}>
        <div>
          <h1 style={{ fontSize: 24, fontWeight: 700, color: '#1e293b' }}>📦 Manual Products</h1>
          <p style={{ fontSize: 14, color: '#64748b' }}>Products with manually-managed credentials</p>
        </div>
      </div>

      <div className="admin-card" style={{ marginBottom: 20 }}>
        <div className="admin-card-body" style={{ padding: 16 }}>
          <div className="admin-search">
            <input
              type="text"
              className="admin-search-input"
              placeholder="Search products..."
              value={search}
              onChange={e => setSearch(e.target.value)}
            />
          </div>
        </div>
      </div>

      <div className="admin-card">
        {loading ? (
          <div className="admin-loading">
            <div className="admin-spinner"></div>
            Loading products...
          </div>
        ) : (
          <div className="admin-table-wrap">
            <table className="admin-table">
              <thead>
                <tr>
                  <th>Product Name</th>
                  <th>Type</th>
                  <th>Total</th>
                  <th>Available</th>
                  <th>Used</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {products.length === 0 ? (
                  <tr>
                    <td colSpan="7">
                      <div className="admin-empty">
                        <div className="admin-empty-icon">📦</div>
                        <div className="admin-empty-text">
                          No manual products found.
                          <br />
                          <span style={{ fontSize: 13, color: '#94a3b8' }}>
                            Mark a product as "manual" in Django Admin to see it here.
                          </span>
                        </div>
                      </div>
                    </td>
                  </tr>
                ) : (
                  products.map(p => (
                    <tr key={p.id}>
                      <td data-label="Product Name" style={{ fontWeight: 600 }}>{p.name}</td>
                      <td data-label="Type">
                        <span className={`admin-badge ${p.credential_type === 'username_password' ? 'blue' : 'amber'}`}>
                          {typeLabel(p.credential_type)}
                        </span>
                      </td>
                      <td data-label="Total" style={{ fontWeight: 500 }}>{p.total_credentials || 0}</td>
                      <td data-label="Available">
                        <span style={{ color: '#059669', fontWeight: 600 }}>
                          {p.available_credentials || 0}
                        </span>
                      </td>
                      <td data-label="Used" style={{ color: '#64748b' }}>{p.used_credentials || 0}</td>
                      <td data-label="Actions">
                        <Link to={`/admin/products/${p.id}`} className="admin-btn admin-btn-primary admin-btn-sm">
                          Manage
                        </Link>
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
