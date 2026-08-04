import { Link, useNavigate } from 'react-router-dom';
import { useState, useEffect } from 'react';
import { useAuth } from '../context/AuthContext';
import api from '../lib/axios';

export default function Layout({ children }) {
  const { user, logout } = useAuth();
  const navigate = useNavigate();
  const [mockEnabled, setMockEnabled] = useState(false);
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);

  useEffect(() => {
    api.get('/dev/mock-status/').then(r => setMockEnabled(r.data.mock_enabled)).catch(() => {});
  }, []);

  const toggleMock = async () => {
    const next = !mockEnabled;
    await api.post('/dev/toggle-mock/', { enabled: next });
    setMockEnabled(next);
  };

  const handleLogout = async () => {
    await logout();
    navigate('/login');
  };

  return (
    <div className="min-h-screen bg-gray-100">
      <nav className="bg-white shadow-sm border-b">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
          <div className="flex justify-between h-16">
            <div className="flex items-center space-x-4">
              <Link to="/catalog" className="text-xl font-bold text-indigo-600">
                RCC
              </Link>
              <div className="hidden md:flex items-center space-x-1">
                <Link to="/catalog" className="text-gray-700 hover:text-indigo-600 px-3 py-2 text-sm">
                  Catalog
                </Link>
                <Link to="/products" className="text-gray-700 hover:text-indigo-600 px-3 py-2 text-sm">
                  Products
                </Link>
                <Link to="/orders" className="text-gray-700 hover:text-indigo-600 px-3 py-2 text-sm">
                  Orders
                </Link>
                {user?.role === 'ADMIN' && (
                  <Link to="/admin" className="text-indigo-600 hover:text-indigo-800 px-3 py-2 text-sm font-semibold">
                    ⚡ Admin Panel
                  </Link>
                )}
              </div>
            </div>
            <div className="hidden md:flex items-center space-x-4">
              <label className="flex items-center space-x-1 text-xs text-gray-500 cursor-pointer select-none">
                <span>Mock</span>
                <input
                  type="checkbox"
                  checked={mockEnabled}
                  onChange={toggleMock}
                  className="rounded"
                />
              </label>
              {user && (
                <span className="text-sm text-gray-600">
                  {user.username}
                </span>
              )}
              <button
                onClick={handleLogout}
                className="text-sm text-red-600 hover:text-red-800"
              >
                Logout
              </button>
            </div>
            <button
              onClick={() => setMobileMenuOpen(true)}
              className="md:hidden p-2 text-gray-600 hover:text-gray-900"
              aria-label="Open menu"
            >
              <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
              </svg>
            </button>
          </div>
        </div>
      </nav>

      {mobileMenuOpen && (
        <div className="fixed inset-0 z-50 md:hidden">
          <div className="fixed inset-0 bg-black/50" onClick={() => setMobileMenuOpen(false)} />
          <div className="fixed top-0 right-0 bottom-0 w-64 bg-white shadow-xl flex flex-col">
            <div className="flex items-center justify-between px-4 h-16 border-b">
              <span className="text-lg font-bold text-indigo-600">Menu</span>
              <button
                onClick={() => setMobileMenuOpen(false)}
                className="p-2 text-gray-600 hover:text-gray-900"
                aria-label="Close menu"
              >
                <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            <div className="flex-1 overflow-y-auto px-4 py-4 space-y-1">
              <Link
                to="/catalog"
                onClick={() => setMobileMenuOpen(false)}
                className="block px-3 py-2 text-gray-700 hover:bg-gray-100 rounded text-sm"
              >
                Catalog
              </Link>
              <Link
                to="/products"
                onClick={() => setMobileMenuOpen(false)}
                className="block px-3 py-2 text-gray-700 hover:bg-gray-100 rounded text-sm"
              >
                Products
              </Link>
              <Link
                to="/orders"
                onClick={() => setMobileMenuOpen(false)}
                className="block px-3 py-2 text-gray-700 hover:bg-gray-100 rounded text-sm"
              >
                Orders
              </Link>
              {user?.role === 'ADMIN' && (
                <Link
                  to="/admin"
                  onClick={() => setMobileMenuOpen(false)}
                  className="block px-3 py-2 text-indigo-600 hover:bg-indigo-50 rounded text-sm font-semibold"
                >
                  ⚡ Admin Panel
                </Link>
              )}
            </div>
            <div className="border-t px-4 py-4 space-y-3">
              <label className="flex items-center space-x-2 text-sm text-gray-500 cursor-pointer select-none">
                <input
                  type="checkbox"
                  checked={mockEnabled}
                  onChange={toggleMock}
                  className="rounded"
                />
                <span>Mock Mode</span>
              </label>
              {user && (
                <p className="text-sm text-gray-600">{user.username}</p>
              )}
              <button
                onClick={handleLogout}
                className="w-full px-3 py-2 text-sm font-medium text-red-600 bg-red-50 rounded hover:bg-red-100"
              >
                Logout
              </button>
            </div>
          </div>
        </div>
      )}

      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {children}
      </main>
    </div>
  );
}
