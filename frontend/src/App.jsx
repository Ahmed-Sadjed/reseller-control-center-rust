import { lazy, Suspense } from 'react';
import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { AuthProvider, useAuth } from './context/AuthContext';
import { ToastProvider } from './context/ToastContext';

const LoginPage = lazy(() => import('./pages/LoginPage'));
const DashboardPage = lazy(() => import('./pages/DashboardPage'));
const CatalogPage = lazy(() => import('./pages/CatalogPage'));
const ProductsPage = lazy(() => import('./pages/ProductsPage'));
const ReceiptPage = lazy(() => import('./pages/ReceiptPage'));
const ProcessingPage = lazy(() => import('./pages/ProcessingPage'));
const OrdersHistory = lazy(() => import('./pages/OrdersHistory'));

const AdminDashboard = lazy(() => import('./pages/admin/AdminDashboard'));
const AdminResellers = lazy(() => import('./pages/admin/AdminResellers'));
const AdminResellerDetail = lazy(() => import('./pages/admin/AdminResellerDetail'));
const AdminProducts = lazy(() => import('./pages/admin/AdminProducts'));
const AdminProductDetail = lazy(() => import('./pages/admin/AdminProductDetail'));
const AdminActivationCodes = lazy(() => import('./pages/admin/AdminActivationCodes'));
const AdminProviders = lazy(() => import('./pages/admin/AdminProviders'));
const AdminWhatsAppOrders = lazy(() => import('./pages/admin/AdminWhatsAppOrders'));
const AdminSettings = lazy(() => import('./pages/admin/AdminSettings'));

function ProtectedRoute({ children }) {
  const { user, loading } = useAuth();
  if (loading) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-gray-100">
        <div className="text-gray-500">Loading...</div>
      </div>
    );
  }
  if (!user) {
    return <Navigate to="/login" replace />;
  }
  return children;
}

function AdminRoute({ children }) {
  const { user, loading } = useAuth();
  if (loading) {
    return (
      <div className="admin-loading">
        <div className="admin-spinner"></div>
        Loading...
      </div>
    );
  }
  if (!user) {
    return <Navigate to="/login" replace />;
  }
  if (user.role !== 'ADMIN') {
    return <Navigate to="/catalog" replace />;
  }
  return children;
}

function PublicRoute({ children }) {
  const { user, loading } = useAuth();
  if (loading) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-gray-100">
        <div className="text-gray-500">Loading...</div>
      </div>
    );
  }
  if (user) {
    return <Navigate to="/catalog" replace />;
  }
  return children;
}

export default function App() {
  return (
    <BrowserRouter>
      <AuthProvider>
        <ToastProvider>
        <Suspense fallback={<div className="min-h-screen flex items-center justify-center bg-gray-100"><div className="text-gray-500">Loading...</div></div>}>
        <Routes>
          <Route
            path="/login"
            element={
              <PublicRoute>
                <LoginPage />
              </PublicRoute>
            }
          />
          <Route
            path="/dashboard"
            element={
              <ProtectedRoute>
                <DashboardPage />
              </ProtectedRoute>
            }
          />
          <Route
            path="/catalog"
            element={
              <ProtectedRoute>
                <CatalogPage />
              </ProtectedRoute>
            }
          />
          <Route
            path="/products"
            element={
              <ProtectedRoute>
                <ProductsPage />
              </ProtectedRoute>
            }
          />
          <Route
            path="/receipt/:orderId"
            element={
              <ProtectedRoute>
                <ReceiptPage />
              </ProtectedRoute>
            }
          />
          <Route
            path="/processing/:orderId"
            element={
              <ProtectedRoute>
                <ProcessingPage />
              </ProtectedRoute>
            }
          />
          <Route
            path="/orders"
            element={
              <ProtectedRoute>
                <OrdersHistory />
              </ProtectedRoute>
            }
          />
          {/* ── Admin Routes ── */}
          <Route
            path="/admin"
            element={
              <AdminRoute>
                <AdminDashboard />
              </AdminRoute>
            }
          />
          <Route
            path="/admin/resellers"
            element={
              <AdminRoute>
                <AdminResellers />
              </AdminRoute>
            }
          />
          <Route
            path="/admin/resellers/:id"
            element={
              <AdminRoute>
                <AdminResellerDetail />
              </AdminRoute>
            }
          />
          <Route
            path="/admin/products"
            element={
              <AdminRoute>
                <AdminProducts />
              </AdminRoute>
            }
          />
          <Route
            path="/admin/products/:id"
            element={
              <AdminRoute>
                <AdminProductDetail />
              </AdminRoute>
            }
          />
          <Route
            path="/admin/codes"
            element={
              <AdminRoute>
                <AdminActivationCodes />
              </AdminRoute>
            }
          />
          <Route
            path="/admin/whatsapp"
            element={
              <AdminRoute>
                <AdminWhatsAppOrders />
              </AdminRoute>
            }
          />
          <Route
            path="/admin/providers"
            element={
              <AdminRoute>
                <AdminProviders />
              </AdminRoute>
            }
          />
          <Route
            path="/admin/settings"
            element={
              <AdminRoute>
                <AdminSettings />
              </AdminRoute>
            }
          />

          <Route path="/" element={<Navigate to="/catalog" replace />} />
          <Route path="*" element={<Navigate to="/catalog" replace />} />
        </Routes>
        </Suspense>
        </ToastProvider>
      </AuthProvider>
    </BrowserRouter>
  );
}
