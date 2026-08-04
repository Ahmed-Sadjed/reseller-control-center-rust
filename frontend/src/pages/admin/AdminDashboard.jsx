import { useState, useEffect } from 'react';
import { Link } from 'react-router-dom';
import AdminLayout from '../../components/AdminLayout';
import api from '../../lib/axios';

function formatNumber(n) {
  if (n == null) return '0';
  return Number(n).toLocaleString('en-US');
}

function formatCredits(n) {
  if (n == null) return '0.00';
  return Number(n).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

function timeAgo(dateStr) {
  const diff = Date.now() - new Date(dateStr).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'Just now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

export default function AdminDashboard() {
  const [stats, setStats] = useState(null);
  const [topResellers, setTopResellers] = useState([]);
  const [recentActivity, setRecentActivity] = useState([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([
      api.get('/dashboard/stats/'),
      api.get('/dashboard/top-resellers/?limit=5'),
      api.get('/dashboard/recent-activity/?limit=10'),
    ])
      .then(([statsRes, topRes, activityRes]) => {
        setStats(statsRes.data);
        setTopResellers(topRes.data);
        setRecentActivity(activityRes.data);
      })
      .catch(console.error)
      .finally(() => setLoading(false));
  }, []);

  if (loading) {
    return (
      <AdminLayout>
        <div className="admin-loading">
          <div className="admin-spinner"></div>
          Loading dashboard...
        </div>
      </AdminLayout>
    );
  }

  return (
    <AdminLayout>
      <div style={{ marginBottom: 8 }}>
        <h1 style={{ fontSize: 24, fontWeight: 700, color: '#1e293b', marginBottom: 4 }}>
          Dashboard Overview
        </h1>
        <p style={{ fontSize: 14, color: '#64748b' }}>
          Monitor your platform at a glance
        </p>
      </div>

      {/* Stat Cards */}
      <div className="admin-stats-grid">
        <div className="admin-stat-card purple">
          <div className="admin-stat-label">Total Resellers</div>
          <div className="admin-stat-value">{formatNumber(stats?.total_resellers)}</div>
          <div className="admin-stat-icon">👥</div>
          <div style={{ fontSize: 12, color: '#10b981', marginTop: 4 }}>
            {formatNumber(stats?.active_resellers)} active
          </div>
        </div>
        <div className="admin-stat-card blue">
          <div className="admin-stat-label">Total Orders</div>
          <div className="admin-stat-value">{formatNumber(stats?.total_orders)}</div>
          <div className="admin-stat-icon">📦</div>
          <div style={{ fontSize: 12, color: '#10b981', marginTop: 4 }}>
            {formatNumber(stats?.completed_orders)} completed
          </div>
        </div>
        <div className="admin-stat-card green">
          <div className="admin-stat-label">Revenue (Credits)</div>
          <div className="admin-stat-value">{formatCredits(stats?.total_revenue)}</div>
          <div className="admin-stat-icon">💰</div>
        </div>
        <div className="admin-stat-card amber">
          <div className="admin-stat-label">Available Codes</div>
          <div className="admin-stat-value">{formatNumber(stats?.available_credentials)}</div>
          <div className="admin-stat-icon">🔑</div>
          <div style={{ fontSize: 12, color: '#64748b', marginTop: 4 }}>
            of {formatNumber(stats?.total_credentials)} total
          </div>
        </div>
      </div>

      <div style={{ marginBottom: 28 }}>
        {/* Top Resellers */}
        <div className="admin-card">
          <div className="admin-card-header">
            <div className="admin-card-title">🏆 Top Resellers</div>
            <Link to="/admin/resellers" className="admin-btn admin-btn-secondary admin-btn-sm">
              View All
            </Link>
          </div>
          <div className="admin-table-wrap">
            <table className="admin-table">
              <thead>
                <tr>
                  <th>#</th>
                  <th>Username</th>
                  <th>Revenue</th>
                  <th>Orders</th>
                  <th>Status</th>
                </tr>
              </thead>
              <tbody>
                {topResellers.length === 0 ? (
                  <tr>
                    <td colSpan="5" style={{ textAlign: 'center', color: '#94a3b8', padding: 32 }}>
                      No resellers yet
                    </td>
                  </tr>
                ) : (
                  topResellers.map((r, i) => (
                    <tr key={r.id}>
                      <td data-label="#" style={{ fontWeight: 600, color: '#6366f1' }}>{i + 1}</td>
                      <td data-label="Username">
                        <Link to={`/admin/resellers/${r.id}`} style={{ color: '#1e293b', fontWeight: 500, textDecoration: 'none' }}>
                          {r.username}
                        </Link>
                      </td>
                      <td data-label="Revenue" style={{ fontWeight: 600 }}>{formatCredits(r.total_revenue)}</td>
                      <td data-label="Orders">{formatNumber(r.order_count)}</td>
                      <td data-label="Status">
                        <span className={`admin-badge ${r.is_active ? 'green' : 'red'}`}>
                          {r.is_active ? '● Active' : '● Inactive'}
                        </span>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      {/* Recent Activity */}
      <div className="admin-card" style={{ marginBottom: 28 }}>
        <div className="admin-card-header">
          <div className="admin-card-title">⏳ Recent Activity</div>
        </div>
        <div className="admin-card-body" style={{ padding: '8px 24px' }}>
          {recentActivity.length === 0 ? (
            <div className="admin-empty">
              <div className="admin-empty-icon">📭</div>
              <div className="admin-empty-text">No activity yet</div>
            </div>
          ) : (
            recentActivity.map((a, i) => (
              <div key={i} className="admin-activity-item">
                <div className={`admin-activity-icon ${a.type}`}>
                  {a.type === 'order' ? '📦' : '💳'}
                </div>
                <div className="admin-activity-body">
                  <div className="admin-activity-text">{a.description}</div>
                  <div className="admin-activity-meta">
                    {a.user} · {a.amount != null ? `${Number(a.amount) >= 0 ? '+' : ''}${formatCredits(a.amount)} credits` : ''}
                    {a.status ? ` · ${a.status}` : ''}
                  </div>
                </div>
                <div style={{ fontSize: 12, color: '#94a3b8', whiteSpace: 'nowrap' }}>
                  {timeAgo(a.created_at)}
                </div>
              </div>
            ))
          )}
        </div>
      </div>

      {/* Quick Actions */}
      <div className="admin-card">
        <div className="admin-card-header">
          <div className="admin-card-title">🚀 Quick Actions</div>
        </div>
        <div className="admin-card-body">
          <div className="admin-actions">
            <Link to="/admin/resellers" className="admin-btn admin-btn-primary">
              ➕ Add Reseller
            </Link>
            <Link to="/admin/products" className="admin-btn admin-btn-secondary">
              📦 Manage Products
            </Link>
            <Link to="/admin/codes" className="admin-btn admin-btn-secondary">
              🔑 View Codes
            </Link>
          </div>
        </div>
      </div>
    </AdminLayout>
  );
}
