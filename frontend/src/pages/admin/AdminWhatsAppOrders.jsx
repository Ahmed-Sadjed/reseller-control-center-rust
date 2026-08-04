import { useState, useEffect } from 'react';
import AdminLayout from '../../components/AdminLayout';
import TableSkeleton from '../../components/skeletons/TableSkeleton';
import api from '../../lib/axios';

export default function AdminWhatsAppOrders() {
  const [orders, setOrders] = useState([]);
  const [loading, setLoading] = useState(true);

  const fetchOrders = async () => {
    setLoading(true);
    try {
      const { data } = await api.get('/dashboard/whatsapp-orders/');
      setOrders(data.results || data);
    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { fetchOrders(); }, []);

  const handleSendWhatsApp = (link) => {
    window.open(link, '_blank');
  };

  const handleComplete = async (uuid) => {
    if (!window.confirm('Mark this order as completed?')) return;
    try {
      await api.post(`/dashboard/whatsapp-orders/${uuid}/complete/`);
      setOrders(prev => prev.filter(o => o.uuid !== uuid));
    } catch (err) {
      console.error(err);
    }
  };

  return (
    <AdminLayout>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 24 }}>
        <div>
          <h1 style={{ fontSize: 24, fontWeight: 700, color: '#1e293b' }}>💬 WhatsApp Orders</h1>
          <p style={{ fontSize: 14, color: '#64748b' }}>Pending orders awaiting WhatsApp notification</p>
        </div>
      </div>

      <div className="admin-card">
        {loading ? (
          <TableSkeleton rows={4} cols={8} columnWidths={['100px', '120px', '80px', '40px', '60px', '120px', '120px', '160px']} />
        ) : (
          <div className="admin-table-wrap">
            <table className="admin-table">
              <thead>
                <tr>
                  <th>Reseller</th>
                  <th>Product</th>
                  <th>Duration</th>
                  <th>Qty</th>
                  <th>Total</th>
                  <th>Ordered</th>
                  <th>Message</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {orders.length === 0 ? (
                  <tr>
                    <td colSpan="8">
                      <div className="admin-empty">
                        <div className="admin-empty-icon">💬</div>
                        <div className="admin-empty-text">No pending WhatsApp orders</div>
                      </div>
                    </td>
                  </tr>
                ) : (
                  orders.map(o => (
                    <tr key={o.uuid}>
                      <td data-label="Reseller" style={{ fontWeight: 500 }}>{o.reseller_username}</td>
                      <td data-label="Product">{o.product_name}</td>
                      <td data-label="Duration" style={{ fontSize: 13, color: '#64748b' }}>{o.duration_display}</td>
                      <td data-label="Qty">{o.quantity}</td>
                      <td data-label="Total" style={{ fontWeight: 500 }}>{o.total_credits}</td>
                      <td data-label="Ordered" style={{ fontSize: 13, color: '#64748b' }}>
                        {new Date(o.created_at).toLocaleString()}
                      </td>
                      <td data-label="Message" style={{ maxWidth: 200, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', fontSize: 12, color: '#64748b' }}>
                        {o.message_text || '—'}
                      </td>
                      <td data-label="Actions">
                        <div style={{ display: 'flex', gap: 8 }}>
                          <button
                            className="admin-btn admin-btn-primary admin-btn-sm"
                            onClick={() => handleSendWhatsApp(o.wa_link)}
                            disabled={!o.wa_link}
                          >
                            💬 Send WhatsApp
                          </button>
                          <button
                            className="admin-btn admin-btn-success admin-btn-sm"
                            onClick={() => handleComplete(o.uuid)}
                          >
                            ✅ Complete
                          </button>
                        </div>
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
