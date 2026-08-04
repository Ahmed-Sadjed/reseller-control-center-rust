import { useState, useEffect } from 'react';
import AdminLayout from '../../components/AdminLayout';
import { useToast } from '../../context/ToastContext';
import api from '../../lib/axios';

export default function AdminSettings() {
  const [whatsappPhone, setWhatsappPhone] = useState('');
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const { addToast } = useToast();

  useEffect(() => {
    api.get('/dashboard/settings/')
      .then(res => setWhatsappPhone(res.data.whatsapp_phone || ''))
      .catch(() => addToast('Failed to load settings.', 'error'))
      .finally(() => setLoading(false));
  }, []);

  const showAlert = (msg, type = 'success') => {
    setAlert({ msg, type });
    setTimeout(() => setAlert(null), 3000);
  };

  const handleSave = async (e) => {
    e.preventDefault();
    setSaving(true);
    try {
      await api.put('/dashboard/settings/', { whatsapp_phone: whatsappPhone });
      addToast('WhatsApp number saved!', 'success');
    } catch {
      addToast('Failed to save.', 'error');
    } finally {
      setSaving(false);
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
              <div className="h-10 w-72 bg-gray-200 animate-pulse rounded" />
              <div className="h-9 w-20 bg-gray-200 animate-pulse rounded" />
            </div>
          </div>
        </div>
      </AdminLayout>
    );
  }

  return (
    <AdminLayout>
      <h1 style={{ fontSize: 24, fontWeight: 700, color: '#1e293b', marginBottom: 8 }}>⚙️ Settings</h1>
      <p style={{ fontSize: 14, color: '#64748b', marginBottom: 24 }}>Configure your admin preferences</p>

      <div className="admin-card">
        <div className="admin-card-body">
          <form onSubmit={handleSave}>
            <h2 style={{ fontSize: 16, fontWeight: 600, color: '#1e293b', marginBottom: 16 }}>💬 WhatsApp Notifications</h2>
            <p style={{ fontSize: 13, color: '#64748b', marginBottom: 16 }}>
              When a WhatsApp product is purchased, a pre-filled message link will be generated using this number.
            </p>
            <div className="admin-field">
              <label className="admin-label">Your WhatsApp Number</label>
              <input
                type="text"
                className="admin-input"
                placeholder="e.g. 2126XXXXXXX"
                value={whatsappPhone}
                onChange={e => setWhatsappPhone(e.target.value)}
                style={{ maxWidth: 300 }}
              />
              <p style={{ fontSize: 12, color: '#94a3b8', marginTop: 4 }}>
                Digits only with country code. You can include + and spaces — they'll be stripped automatically. Example: 213556471889
              </p>
            </div>
            <button type="submit" className="admin-btn admin-btn-primary" disabled={saving}>
              {saving ? 'Saving...' : 'Save'}
            </button>
          </form>
        </div>
      </div>
    </AdminLayout>
  );
}
