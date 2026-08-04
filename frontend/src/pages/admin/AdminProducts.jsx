import { useState, useEffect, useCallback } from 'react';
import { Link } from 'react-router-dom';
import AdminLayout from '../../components/AdminLayout';
import TableSkeleton from '../../components/skeletons/TableSkeleton';
import { useToast } from '../../context/ToastContext';
import api from '../../lib/axios';

const TYPE_LABELS = {
  manual: { label: 'Manual', class: 'amber' },
  api: { label: 'API', class: 'blue' },
  whatsapp: { label: 'WhatsApp', class: 'green' },
};

const DURATION_LABELS = {
  100: '6 Hours', 101: '12 Hours', 102: '24 Hours', 103: '72 Hours',
  1: '1 Month', 3: '3 Months', 6: '6 Months', 12: '12 Months',
  15: '15 Months', 24: '2 Years', 36: '3 Years',
};

export default function AdminProducts() {
  const [products, setProducts] = useState([]);
  const [categories, setCategories] = useState([]);
  const [providers, setProviders] = useState([]);
  const [loading, setLoading] = useState(true);

  // Filters
  const [search, setSearch] = useState('');
  const [filterType, setFilterType] = useState('');
  const [filterCategory, setFilterCategory] = useState('');
  const [filterStatus, setFilterStatus] = useState('');
  const [page, setPage] = useState(1);
  const [pagination, setPagination] = useState({ count: 0 });

  // Product Modal
  const [showProductModal, setShowProductModal] = useState(false);
  const [editingProduct, setEditingProduct] = useState(null);
  const [productForm, setProductForm] = useState({
    name: '', category: '', provider: '', description: '',
    is_manual: false, credential_type: '', is_active: true,
  });
  const [productImage, setProductImage] = useState(null);
  const [productImagePreview, setProductImagePreview] = useState(null);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState('');
  const [productVariants, setProductVariants] = useState([]);
  const [removedVariantIds, setRemovedVariantIds] = useState([]);

  // Delete confirmation
  const [deleteTarget, setDeleteTarget] = useState(null);
  const [deleting, setDeleting] = useState(false);

  // Variant modal
  const [showVariantModal, setShowVariantModal] = useState(false);
  const [variantProduct, setVariantProduct] = useState(null);
  const [variants, setVariants] = useState([]);
  const [editingVariant, setEditingVariant] = useState(null);
  const [variantForm, setVariantForm] = useState({
    duration_months: '', is_lifetime: false, external_pack_id: '',
    price_in_credits: '', is_active: true,
  });
  const [savingVariant, setSavingVariant] = useState(false);

  // Category modal
  const [showCategoryModal, setShowCategoryModal] = useState(false);
  const [editingCategory, setEditingCategory] = useState(null);
  const [categoryForm, setCategoryForm] = useState({
    name: '', description: '', is_active: true, sort_order: 0,
  });
  const [categoryImage, setCategoryImage] = useState(null);
  const [savingCategory, setSavingCategory] = useState(false);

  // Alert
  const { addToast } = useToast();

  const fetchProducts = useCallback(async () => {
    setLoading(true);
    try {
      const params = new URLSearchParams();
      params.set('page', page);
      if (search) params.set('search', search);
      if (filterType) params.set('type', filterType);
      if (filterCategory) params.set('category', filterCategory);
      if (filterStatus) params.set('status', filterStatus);

      const { data } = await api.get(`/dashboard/products/?${params}`);
      setProducts(data.results || data);
      setPagination({ count: data.count || 0, next: data.next, previous: data.previous });
    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  }, [page, search, filterType, filterCategory, filterStatus]);

  const fetchCategories = useCallback(async () => {
    try {
      const { data } = await api.get('/dashboard/categories/');
      setCategories(data);
    } catch (err) {
      console.error(err);
    }
  }, []);

  const fetchProviders = useCallback(async () => {
    try {
      const { data } = await api.get('/dashboard/providers/');
      setProviders(data);
    } catch (err) {
      console.error(err);
    }
  }, []);

  useEffect(() => {
    fetchProducts();
    fetchCategories();
    fetchProviders();
  }, [fetchProducts, fetchCategories, fetchProviders]);

  useEffect(() => { setPage(1); }, [search, filterType, filterCategory, filterStatus]);

  // ── Product CRUD ──

  const openAddProduct = () => {
    setEditingProduct(null);
    setProductForm({
      name: '', category: '', provider: '', description: '',
      is_manual: false, credential_type: '', is_active: true,
    });
    setProductVariants([]);
    setRemovedVariantIds([]);
    setProductImage(null);
    setProductImagePreview(null);
    setFormError('');
    setShowProductModal(true);
  };

  const openEditProduct = (product) => {
    setEditingProduct(product);
    setProductForm({
      name: product.name || '',
      category: product.category || '',
      provider: product.provider || '',
      description: product.description || '',
      is_manual: product.is_manual || false,
      credential_type: product.credential_type || '',
      is_active: product.is_active,
    });
    setProductVariants(product.variants || []);
    setRemovedVariantIds([]);
    setProductImage(null);
    setProductImagePreview(product.image_url || null);
    setFormError('');
    setShowProductModal(true);
  };

  const handleProductImageChange = (e) => {
    const file = e.target.files[0];
    if (file) {
      setProductImage(file);
      setProductImagePreview(URL.createObjectURL(file));
    }
  };

  // ── Inline Variant Row Helpers ──

  const addProductVariantRow = () => {
    setProductVariants(prev => [...prev, { duration_months: '', price_in_credits: '', is_lifetime: false, external_pack_id: '' }]);
  };

  const updateProductVariantRow = (index, field, value) => {
    setProductVariants(prev => {
      const updated = [...prev];
      updated[index] = { ...updated[index], [field]: value };
      if (field === 'is_lifetime' && value) {
        updated[index].duration_months = '';
      }
      return updated;
    });
  };

  const removeProductVariantRow = (index) => {
    const variant = productVariants[index];
    if (variant.id) {
      setRemovedVariantIds(prev => [...prev, variant.id]);
    }
    setProductVariants(prev => prev.filter((_, i) => i !== index));
  };

  const handleSaveProduct = async (e) => {
    e.preventDefault();
    setSaving(true);
    setFormError('');

    try {
      const formData = new FormData();
      Object.entries(productForm).forEach(([key, value]) => {
        if (value !== '' && value !== null) {
          formData.append(key, value);
        }
      });
      if (productImage) {
        formData.append('image', productImage);
      }

      let productId;

      if (editingProduct) {
        await api.put(`/dashboard/products/${editingProduct.id}/`, formData);
        productId = editingProduct.id;

        for (const vid of removedVariantIds) {
          await api.delete(`/dashboard/products/${productId}/variants/${vid}/`);
        }

        for (const variant of productVariants) {
          const payload = {
            duration_months: variant.is_lifetime ? null : (Number(variant.duration_months) || null),
            price_in_credits: Number(variant.price_in_credits) || 0,
            is_lifetime: !!variant.is_lifetime,
            external_pack_id: variant.external_pack_id ? Number(variant.external_pack_id) : null,
            is_active: true,
          };
          if (variant.id) {
            await api.put(`/dashboard/products/${productId}/variants/${variant.id}/`, payload);
          } else {
            await api.post(`/dashboard/products/${productId}/variants/create/`, payload);
          }
        }

        addToast('Product updated successfully!', 'success');
      } else {
        const { data } = await api.post('/dashboard/products/create/', formData);
        productId = data.id;

        for (const variant of productVariants) {
          const payload = {
            duration_months: variant.is_lifetime ? null : (Number(variant.duration_months) || null),
            price_in_credits: Number(variant.price_in_credits) || 0,
            is_lifetime: !!variant.is_lifetime,
            external_pack_id: variant.external_pack_id ? Number(variant.external_pack_id) : null,
            is_active: true,
          };
          await api.post(`/dashboard/products/${productId}/variants/create/`, payload);
        }

        addToast('Product created successfully!', 'success');
      }

      setShowProductModal(false);
      fetchProducts();
    } catch (err) {
      const data = err.response?.data;
      if (typeof data === 'string') {
        setFormError(data);
      } else {
        setFormError(Object.values(data || {}).flat().join(', '));
      }
    } finally {
      setSaving(false);
    }
  };

  const handleDeleteProduct = async () => {
    if (!deleteTarget) return;
    setDeleting(true);
    try {
      await api.delete(`/dashboard/products/${deleteTarget.id}/`);
      addToast(`Product "${deleteTarget.name}" deleted.`, 'success');
      setDeleteTarget(null);
      fetchProducts();
    } catch (err) {
      addToast('Failed to delete product.', 'error');
    } finally {
      setDeleting(false);
    }
  };

  // ── Variant CRUD ──

  const openVariantManager = async (product) => {
    setVariantProduct(product);
    setEditingVariant(null);
    setVariantForm({
      duration_months: '', is_lifetime: false, external_pack_id: '',
      price_in_credits: '', is_active: true,
    });
    try {
      const { data } = await api.get(`/dashboard/products/${product.id}/variants/`);
      setVariants(data);
    } catch (err) {
      setVariants([]);
    }
    setShowVariantModal(true);
  };

  const openEditVariant = (variant) => {
    setEditingVariant(variant);
    setVariantForm({
      duration_months: variant.duration_months || '',
      is_lifetime: variant.is_lifetime,
      external_pack_id: variant.external_pack_id || '',
      price_in_credits: variant.price_in_credits || '',
      is_active: variant.is_active,
    });
  };

  const handleSaveVariant = async (e) => {
    e.preventDefault();
    setSavingVariant(true);
    try {
      const payload = {
        ...variantForm,
        external_pack_id: Number(variantForm.external_pack_id) || 0,
        price_in_credits: Number(variantForm.price_in_credits) || 0,
        duration_months: variantForm.is_lifetime ? null : (Number(variantForm.duration_months) || null),
      };

      if (editingVariant?.id) {
        await api.put(`/dashboard/products/${variantProduct.id}/variants/${editingVariant.id}/`, payload);
        addToast('Variant updated!', 'success');
      } else {
        await api.post(`/dashboard/products/${variantProduct.id}/variants/create/`, payload);
        addToast('Variant added!', 'success');
      }

      setEditingVariant(null);
      setVariantForm({
        duration_months: '', is_lifetime: false, external_pack_id: '',
        price_in_credits: '', is_active: true,
      });

      const { data } = await api.get(`/dashboard/products/${variantProduct.id}/variants/`);
      setVariants(data);
    } catch (err) {
      addToast('Failed to save variant.', 'error');
    } finally {
      setSavingVariant(false);
    }
  };

  const handleDeleteVariant = async (variant) => {
    if (!window.confirm(`Delete variant for ${variantProduct.name}?`)) return;
    try {
      await api.delete(`/dashboard/products/${variantProduct.id}/variants/${variant.id}/`);
      addToast('Variant deleted.', 'success');
      const { data } = await api.get(`/dashboard/products/${variantProduct.id}/variants/`);
      setVariants(data);
    } catch (err) {
      addToast('Failed to delete variant.', 'error');
    }
  };

  // ── Category CRUD ──

  const openAddCategory = () => {
    setEditingCategory(null);
    setCategoryForm({ name: '', description: '', is_active: true, sort_order: 0 });
    setCategoryImage(null);
    setShowCategoryModal(true);
  };

  const openEditCategory = (cat) => {
    setEditingCategory(cat);
    setCategoryForm({
      name: cat.name,
      description: cat.description || '',
      is_active: cat.is_active,
      sort_order: cat.sort_order || 0,
    });
    setCategoryImage(null);
    setShowCategoryModal(true);
  };

  const handleSaveCategory = async (e) => {
    e.preventDefault();
    setSavingCategory(true);
    try {
      const formData = new FormData();
      Object.entries(categoryForm).forEach(([key, value]) => {
        if (value !== '' && value !== null) formData.append(key, value);
      });
      if (categoryImage) formData.append('image', categoryImage);

      if (editingCategory) {
        await api.put(`/dashboard/categories/${editingCategory.id}/`, formData);
        addToast('Category updated!', 'success');
      } else {
        await api.post('/dashboard/categories/create/', formData);
        addToast('Category created!', 'success');
      }

      setShowCategoryModal(false);
      fetchCategories();
    } catch (err) {
      const data = err.response?.data;
      addToast(typeof data === 'string' ? data : 'Failed to save category.', 'error');
    } finally {
      setSavingCategory(false);
    }
  };

  const handleDeleteCategory = async (cat) => {
    if (!window.confirm(`Delete category "${cat.name}"? This cannot be undone if it has no products.`)) return;
    try {
      await api.delete(`/dashboard/categories/${cat.id}/`);
      addToast(`Category "${cat.name}" deleted.`, 'success');
      fetchCategories();
    } catch (err) {
      const data = err.response?.data;
      addToast(data?.error || 'Failed to delete category.', 'error');
    }
  };

  // ── Helpers ──

  const typeLabel = (p) => {
    const t = p.product_type || (p.is_manual ? 'manual' : (p.provider_key === 'whatsapp' ? 'whatsapp' : 'api'));
    return TYPE_LABELS[t] || { label: t, class: 'gray' };
  };

  const credTypeLabel = (t) => {
    if (t === 'username_password') return 'U:P';
    if (t === 'single_code') return 'Code';
    return t || '—';
  };

  const durationLabel = (d) => DURATION_LABELS[d] || (d ? `${d} Months` : '—');

  return (
    <AdminLayout>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 24 }}>
        <div>
          <h1 style={{ fontSize: 24, fontWeight: 700, color: '#1e293b' }}>Products</h1>
          <p style={{ fontSize: 14, color: '#64748b' }}>Manage all products, categories, and variants</p>
        </div>
        <div style={{ display: 'flex', gap: 10 }}>
          <button className="admin-btn admin-btn-success" onClick={openAddCategory}>
            + Category
          </button>
          <button className="admin-btn admin-btn-primary" onClick={openAddProduct}>
            + Add Product
          </button>
        </div>
      </div>

      {/* ── Product Filters ── */}
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
            <select className="admin-select" value={filterType} onChange={e => setFilterType(e.target.value)}>
              <option value="">All Types</option>
              <option value="manual">Manual</option>
              <option value="api">API</option>
              <option value="whatsapp">WhatsApp</option>
            </select>
            <select className="admin-select" value={filterCategory} onChange={e => setFilterCategory(e.target.value)}>
              <option value="">All Categories</option>
              {categories.map(c => (
                <option key={c.id} value={c.id}>{c.name}</option>
              ))}
            </select>
            <select className="admin-select" value={filterStatus} onChange={e => setFilterStatus(e.target.value)}>
              <option value="">All Status</option>
              <option value="active">Active</option>
              <option value="inactive">Inactive</option>
            </select>
          </div>
        </div>
      </div>

      {/* ── Product Table ── */}
      <div className="admin-card">
        {loading ? (
          <TableSkeleton rows={6} cols={9} columnWidths={['40px', '120px', '80px', '100px', '100px', '80px', '70px', '60px', '120px']} />
        ) : (
          <div className="admin-table-wrap">
            <table className="admin-table">
              <thead>
                <tr>
                  <th style={{ width: 50 }}>Img</th>
                  <th>Name</th>
                  <th>Type</th>
                  <th>Category</th>
                  <th>Provider</th>
                  <th>Price</th>
                  <th>Status</th>
                  <th>Variants</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {products.length === 0 ? (
                  <tr>
                    <td colSpan="9">
                      <div className="admin-empty">
                        <div className="admin-empty-icon">📦</div>
                        <div className="admin-empty-text">No products found.</div>
                      </div>
                    </td>
                  </tr>
                ) : (
                  products.map(p => {
                    const t = typeLabel(p);
                    return (
                      <tr key={p.id}>
                        <td data-label="Img">
                          {p.image_url ? (
                            <img src={p.image_url} alt="" style={{ width: 40, height: 40, borderRadius: 6, objectFit: 'cover' }} />
                          ) : (
                            <div style={{ width: 40, height: 40, borderRadius: 6, background: '#f1f5f9', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 18 }}>📦</div>
                          )}
                        </td>
                        <td data-label="Name" style={{ fontWeight: 600 }}>{p.name}</td>
                        <td data-label="Type">
                          <span className={`admin-badge ${t.class}`}>{t.label}</span>
                          {p.is_manual && p.credential_type && (
                            <span style={{ fontSize: 11, color: '#94a3b8', marginLeft: 4 }}>({credTypeLabel(p.credential_type)})</span>
                          )}
                        </td>
                        <td data-label="Category" style={{ color: '#64748b' }}>{p.category_name || '—'}</td>
                        <td data-label="Provider" style={{ color: '#64748b' }}>{p.provider_name || '—'}</td>
                        <td data-label="Price" style={{ fontWeight: 500 }}>
                          {p.variants?.length > 0
                            ? `${Math.min(...p.variants.map(v => Number(v.price_in_credits)))}–${Math.max(...p.variants.map(v => Number(v.price_in_credits)))}`
                            : p.price_in_credits ? `${p.price_in_credits}` : '—'
                          }
</td>
                        <td data-label="Status">
                          <span className={`admin-badge ${p.is_active ? 'green' : 'red'}`}>
                            {p.is_active ? 'Active' : 'Inactive'}
                          </span>
                        </td>
                        <td data-label="Variants" style={{ fontSize: 13, color: '#64748b' }}>{p.variant_count || 0}</td>
                        <td data-label="Actions">
                          <div style={{ display: 'flex', gap: 6 }}>
                            <button className="admin-btn admin-btn-sm admin-btn-secondary" title="Edit" onClick={() => openEditProduct(p)}>✏️</button>
                            <button className="admin-btn admin-btn-sm admin-btn-secondary" title="Variants" onClick={() => openVariantManager(p)}>🔀</button>
                            {p.is_manual && (
                              <Link to={`/admin/products/${p.id}`} className="admin-btn admin-btn-sm admin-btn-primary" title="Credentials">🔑</Link>
                            )}
                            <button className="admin-btn admin-btn-sm admin-btn-danger" title="Delete" onClick={() => setDeleteTarget(p)}>🗑️</button>
                          </div>
                        </td>
                      </tr>
                    );
                  })
                )}
              </tbody>
            </table>
          </div>
        )}

        {pagination.count > 20 && (
          <div className="admin-pagination">
            <button className="admin-page-btn" disabled={!pagination.previous} onClick={() => setPage(p => Math.max(1, p - 1))}>← Prev</button>
            <span className="admin-page-info">Page {page} · {pagination.count} total</span>
            <button className="admin-page-btn" disabled={!pagination.next} onClick={() => setPage(p => p + 1)}>Next →</button>
          </div>
        )}
      </div>

      {/* ── Categories Section ── */}
      <div className="admin-card" style={{ marginTop: 24 }}>
        <div className="admin-card-header">
          <div className="admin-card-title">Categories</div>
          <button className="admin-btn admin-btn-sm admin-btn-primary" onClick={openAddCategory}>+ Add</button>
        </div>
        <div className="admin-table-wrap">
          <table className="admin-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Slug</th>
                <th>Sort</th>
                <th>Products</th>
                <th>Status</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {categories.length === 0 ? (
                <tr>
                  <td colSpan="6">
                    <div className="admin-empty" style={{ padding: '24px' }}>
                      <div className="admin-empty-text">No categories yet.</div>
                    </div>
                  </td>
                </tr>
              ) : (
                categories.map(c => (
                  <tr key={c.id}>
                    <td data-label="Name" style={{ fontWeight: 600 }}>{c.name}</td>
                    <td data-label="Slug" style={{ color: '#64748b', fontFamily: 'monospace', fontSize: 13 }}>{c.slug}</td>
                    <td data-label="Sort">{c.sort_order}</td>
                    <td data-label="Products" style={{ fontWeight: 500 }}>{c.product_count || 0}</td>
                    <td data-label="Status">
                      <span className={`admin-badge ${c.is_active ? 'green' : 'red'}`}>
                        {c.is_active ? 'Active' : 'Inactive'}
                      </span>
                    </td>
                    <td data-label="Actions">
                      <div style={{ display: 'flex', gap: 6 }}>
                        <button className="admin-btn admin-btn-sm admin-btn-secondary" onClick={() => openEditCategory(c)}>✏️</button>
                        <button className="admin-btn admin-btn-sm admin-btn-danger" onClick={() => handleDeleteCategory(c)}>🗑️</button>
                      </div>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </div>

      {/* ── Add/Edit Product Modal ── */}
      {showProductModal && (
        <div className="admin-modal-overlay" onClick={() => setShowProductModal(false)}>
          <div className="admin-modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 600 }}>
            <div className="admin-modal-header">
              <div className="admin-modal-title">{editingProduct ? '✏️ Edit Product' : '➕ Add Product'}</div>
              <button className="admin-modal-close" onClick={() => setShowProductModal(false)}>✕</button>
            </div>
            <form onSubmit={handleSaveProduct}>
              <div className="admin-modal-body">
                {formError && <div className="admin-alert error">⚠ {formError}</div>}

                <div className="admin-field">
                  <label className="admin-label">Product Name *</label>
                  <input className="admin-input" required value={productForm.name} onChange={e => setProductForm(f => ({ ...f, name: e.target.value }))} />
                </div>

                <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 16 }}>
                  <div className="admin-field">
                    <label className="admin-label">Category</label>
                    <select className="admin-select" value={productForm.category} onChange={e => setProductForm(f => ({ ...f, category: e.target.value }))}>
                      <option value="">Select...</option>
                      {categories.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
                    </select>
                  </div>
                  <div className="admin-field">
                    <label className="admin-label">Provider</label>
                    <select className="admin-select" value={productForm.provider} onChange={e => setProductForm(f => ({ ...f, provider: e.target.value }))}>
                      <option value="">Select...</option>
                      {providers.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
                    </select>
                  </div>
                </div>

                <div className="admin-field">
                  <label className="admin-label">Description</label>
                  <textarea className="admin-input" rows={3} value={productForm.description} onChange={e => setProductForm(f => ({ ...f, description: e.target.value }))} />
                </div>

                <div className="admin-field">
                  <label className="admin-label">Product Image</label>
                  <input type="file" accept="image/*" onChange={handleProductImageChange} style={{ fontSize: 14 }} />
                  {productImagePreview && (
                    <img src={productImagePreview} alt="Preview" style={{ marginTop: 8, height: 100, width: 'auto', borderRadius: 6 }} />
                  )}
                </div>

                <div style={{ borderTop: '1px solid #e2e8f0', paddingTop: 16, marginTop: 8 }}>
                  <div style={{ display: 'flex', gap: 16, alignItems: 'center', marginBottom: 16 }}>
                    <label className="admin-label" style={{ marginBottom: 0 }}>Type:</label>
                    <label style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 14, cursor: 'pointer' }}>
                      <input type="radio" name="is_manual" checked={!productForm.is_manual} onChange={() => setProductForm(f => ({ ...f, is_manual: false, credential_type: '' }))} /> API / WhatsApp
                    </label>
                    <label style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 14, cursor: 'pointer' }}>
                      <input type="radio" name="is_manual" checked={productForm.is_manual} onChange={() => setProductForm(f => ({ ...f, is_manual: true }))} /> Manual
                    </label>
                  </div>

                  {productForm.is_manual && (
                    <div className="admin-field">
                      <label className="admin-label">Credential Type *</label>
                      <select className="admin-select" required value={productForm.credential_type} onChange={e => setProductForm(f => ({ ...f, credential_type: e.target.value }))}>
                        <option value="">Select...</option>
                        <option value="username_password">Username + Password</option>
                        <option value="single_code">Single Code</option>
                      </select>
                    </div>
                  )}

                  <div className="admin-field">
                    <label className="admin-label">Variants (Duration & Price)</label>
                    <div style={{ border: '1px solid #e2e8f0', borderRadius: 8, overflow: 'hidden' }}>
                      <table className="admin-table" style={{ margin: 0 }}>
                        <thead>
                          <tr>
                            <th style={{ width: '35%' }}>Duration</th>
                            <th style={{ width: '25%' }}>Price (credits)</th>
                            <th style={{ width: '25%' }}>Pack ID</th>
                            <th style={{ width: '15%' }}></th>
                          </tr>
                        </thead>
                        <tbody>
                          {productVariants.length === 0 ? (
                            <tr>
                              <td colSpan={4} style={{ textAlign: 'center', color: '#94a3b8', padding: 16 }}>
                                No variants yet. Click &quot;Add Variant&quot; below.
                              </td>
                            </tr>
                          ) : (
                            productVariants.map((v, i) => (
                              <tr key={i}>
                                <td>
                                  <select className="admin-select" value={v.duration_months ?? ''} onChange={e => updateProductVariantRow(i, 'duration_months', e.target.value)} disabled={v.is_lifetime} style={{ width: '100%' }}>
                                    <option value="">Lifetime</option>
                                    {Object.entries(DURATION_LABELS).map(([k, label]) => (
                                      <option key={k} value={k}>{label}</option>
                                    ))}
                                  </select>
                                </td>
                                <td>
                                  <input type="number" step="0.01" className="admin-input" value={v.price_in_credits} onChange={e => updateProductVariantRow(i, 'price_in_credits', e.target.value)} style={{ width: '100%' }} placeholder="0.00" />
                                </td>
                                <td>
                                  <input type="number" step="1" className="admin-input" value={v.external_pack_id} onChange={e => updateProductVariantRow(i, 'external_pack_id', e.target.value)} style={{ width: '100%' }} placeholder="Pack ID" />
                                </td>
                                <td>
                                  <button type="button" className="admin-btn admin-btn-sm admin-btn-danger" onClick={() => removeProductVariantRow(i)} title="Remove">✕</button>
                                </td>
                              </tr>
                            ))
                          )}
                        </tbody>
                      </table>
                    </div>
                    <button type="button" className="admin-btn admin-btn-sm admin-btn-secondary" onClick={addProductVariantRow} style={{ marginTop: 8 }}>+ Add Variant</button>
                  </div>
                </div>

                <div className="admin-field" style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                  <label className="admin-label" style={{ marginBottom: 0, cursor: 'pointer' }}>
                    <input type="checkbox" checked={productForm.is_active} onChange={e => setProductForm(f => ({ ...f, is_active: e.target.checked }))} style={{ marginRight: 6 }} />
                    Active
                  </label>
                </div>
              </div>
              <div className="admin-modal-footer">
                <button type="button" className="admin-btn admin-btn-secondary" onClick={() => setShowProductModal(false)}>Cancel</button>
                <button type="submit" className="admin-btn admin-btn-primary" disabled={saving}>
                  {saving ? 'Saving...' : editingProduct ? 'Update Product' : 'Create Product'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* ── Delete Confirmation ── */}
      {deleteTarget && (
        <div className="admin-modal-overlay" onClick={() => setDeleteTarget(null)}>
          <div className="admin-modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 420 }}>
            <div className="admin-modal-header">
              <div className="admin-modal-title" style={{ color: '#ef4444' }}>Delete Product</div>
              <button className="admin-modal-close" onClick={() => setDeleteTarget(null)}>✕</button>
            </div>
            <div className="admin-modal-body">
              <p style={{ fontSize: 14, color: '#475569', lineHeight: 1.6 }}>
                Are you sure you want to delete <strong>{deleteTarget.name}</strong>?
              </p>
              {deleteTarget.variant_count > 0 && (
                <p style={{ fontSize: 13, color: '#f59e0b', marginTop: 8 }}>
                  This product has {deleteTarget.variant_count} variant(s). They will also be deleted.
                </p>
              )}
              <p style={{ fontSize: 13, color: '#94a3b8', marginTop: 8 }}>
                If it has existing orders, it will be deactivated instead of deleted.
              </p>
            </div>
            <div className="admin-modal-footer">
              <button className="admin-btn admin-btn-secondary" onClick={() => setDeleteTarget(null)}>Cancel</button>
              <button className="admin-btn admin-btn-danger" disabled={deleting} onClick={handleDeleteProduct}>
                {deleting ? 'Deleting...' : 'Delete'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ── Variant Manager Modal ── */}
      {showVariantModal && variantProduct && (
        <div className="admin-modal-overlay" onClick={() => setShowVariantModal(false)}>
          <div className="admin-modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 600 }}>
            <div className="admin-modal-header">
              <div className="admin-modal-title">🔀 Variants — {variantProduct.name}</div>
              <button className="admin-modal-close" onClick={() => setShowVariantModal(false)}>✕</button>
            </div>
            <div className="admin-modal-body">
              {!editingVariant ? (
                <div>
                  <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 12 }}>
                    <button className="admin-btn admin-btn-sm admin-btn-primary" onClick={() => setEditingVariant({})}>+ Add Variant</button>
                  </div>
                  {variants.length === 0 ? (
                    <div className="admin-empty" style={{ padding: 24 }}>
                      <div className="admin-empty-text">No variants yet.</div>
                    </div>
                  ) : (
                    <table className="admin-table">
                      <thead>
                        <tr>
                          <th>Duration</th>
                          <th>Pack ID</th>
                          <th>Price</th>
                          <th>Active</th>
                          <th>Actions</th>
                        </tr>
                      </thead>
                      <tbody>
                        {variants.map(v => (
                          <tr key={v.id}>
                            <td>{v.is_lifetime ? 'Lifetime' : durationLabel(v.duration_months)}</td>
                            <td style={{ fontFamily: 'monospace', fontSize: 13 }}>{v.external_pack_id}</td>
                            <td style={{ fontWeight: 500 }}>{v.price_in_credits}</td>
                            <td>
                              <span className={`admin-badge ${v.is_active ? 'green' : 'red'}`}>
                                {v.is_active ? 'Yes' : 'No'}
                              </span>
                            </td>
                            <td>
                              <div style={{ display: 'flex', gap: 6 }}>
                                <button className="admin-btn admin-btn-sm admin-btn-secondary" onClick={() => openEditVariant(v)}>✏️</button>
                                <button className="admin-btn admin-btn-sm admin-btn-danger" onClick={() => handleDeleteVariant(v)}>🗑️</button>
                              </div>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  )}
                </div>
              ) : (
                <form onSubmit={handleSaveVariant}>
                  <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 16 }}>
                    <div className="admin-field">
                      <label className="admin-label">Duration</label>
                      <select className="admin-select" value={variantForm.duration_months} onChange={e => setVariantForm(f => ({ ...f, duration_months: e.target.value }))} disabled={variantForm.is_lifetime}>
                        <option value="">Select...</option>
                        {Object.entries(DURATION_LABELS).map(([k, v]) => (
                          <option key={k} value={k}>{v}</option>
                        ))}
                      </select>
                    </div>
                    <div className="admin-field">
                      <label className="admin-label">External Pack ID *</label>
                      <input type="number" className="admin-input" required value={variantForm.external_pack_id} onChange={e => setVariantForm(f => ({ ...f, external_pack_id: e.target.value }))} />
                    </div>
                  </div>
                  <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 16 }}>
                    <div className="admin-field">
                      <label className="admin-label">Price (credits) *</label>
                      <input type="number" step="0.01" className="admin-input" required value={variantForm.price_in_credits} onChange={e => setVariantForm(f => ({ ...f, price_in_credits: e.target.value }))} />
                    </div>
                    <div className="admin-field" style={{ display: 'flex', alignItems: 'center', gap: 8, paddingTop: 24 }}>
                      <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 14, cursor: 'pointer' }}>
                        <input type="checkbox" checked={variantForm.is_lifetime} onChange={e => setVariantForm(f => ({ ...f, is_lifetime: e.target.checked, duration_months: '' }))} />
                        Lifetime
                      </label>
                      <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 14, cursor: 'pointer' }}>
                        <input type="checkbox" checked={variantForm.is_active} onChange={e => setVariantForm(f => ({ ...f, is_active: e.target.checked }))} />
                        Active
                      </label>
                    </div>
                  </div>
                  <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 16 }}>
                    <button type="button" className="admin-btn admin-btn-secondary" onClick={() => setEditingVariant(null)}>Cancel</button>
                    <button type="submit" className="admin-btn admin-btn-primary" disabled={savingVariant}>
                      {savingVariant ? 'Saving...' : editingVariant?.id ? 'Update Variant' : 'Add Variant'}
                    </button>
                  </div>
                </form>
              )}
            </div>
          </div>
        </div>
      )}

      {/* ── Category Modal ── */}
      {showCategoryModal && (
        <div className="admin-modal-overlay" onClick={() => setShowCategoryModal(false)}>
          <div className="admin-modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 480 }}>
            <div className="admin-modal-header">
              <div className="admin-modal-title">{editingCategory ? '✏️ Edit Category' : '➕ Add Category'}</div>
              <button className="admin-modal-close" onClick={() => setShowCategoryModal(false)}>✕</button>
            </div>
            <form onSubmit={handleSaveCategory}>
              <div className="admin-modal-body">
                <div className="admin-field">
                  <label className="admin-label">Name *</label>
                  <input className="admin-input" required value={categoryForm.name} onChange={e => setCategoryForm(f => ({ ...f, name: e.target.value }))} />
                </div>
                <div className="admin-field">
                  <label className="admin-label">Description</label>
                  <textarea className="admin-input" rows={3} value={categoryForm.description} onChange={e => setCategoryForm(f => ({ ...f, description: e.target.value }))} />
                </div>
                <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 16 }}>
                  <div className="admin-field">
                    <label className="admin-label">Sort Order</label>
                    <input type="number" className="admin-input" value={categoryForm.sort_order} onChange={e => setCategoryForm(f => ({ ...f, sort_order: Number(e.target.value) }))} />
                  </div>
                  <div className="admin-field" style={{ display: 'flex', alignItems: 'center', gap: 8, paddingTop: 24 }}>
                    <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 14, cursor: 'pointer' }}>
                      <input type="checkbox" checked={categoryForm.is_active} onChange={e => setCategoryForm(f => ({ ...f, is_active: e.target.checked }))} />
                      Active
                    </label>
                  </div>
                </div>
                <div className="admin-field">
                  <label className="admin-label">Image</label>
                  <input type="file" accept="image/*" onChange={e => setCategoryImage(e.target.files[0])} style={{ fontSize: 14 }} />
                </div>
              </div>
              <div className="admin-modal-footer">
                <button type="button" className="admin-btn admin-btn-secondary" onClick={() => setShowCategoryModal(false)}>Cancel</button>
                <button type="submit" className="admin-btn admin-btn-primary" disabled={savingCategory}>
                  {savingCategory ? 'Saving...' : editingCategory ? 'Update Category' : 'Create Category'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </AdminLayout>
  );
}