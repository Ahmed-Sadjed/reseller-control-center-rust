import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import api from '../lib/axios';

const MAC_REGEX = /^([0-9A-Za-z]{2}:){5}[0-9A-Za-z]{2}$/;

function generateUUID() {
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0;
    const v = c === 'x' ? r : (r & 0x3) | 0x8;
    return v.toString(16);
  });
}

function getDurationVariant(variants, label) {
  if (label === 'forever') return variants.find((v) => v.display_name === 'Lifetime') || variants[variants.length - 1];
  return variants.find((v) => v.display_name !== 'Lifetime') || variants[0];
}

function formatDuration(variant) {
  return variant.display_name || `${variant.duration_months} Month${variant.duration_months > 1 ? 's' : ''}`;
}

export default function ProductCard({ product, onError }) {
  const [quantity, setQuantity] = useState(1);
  const [buying, setBuying] = useState(false);
  const navigate = useNavigate();

  const variants = product.variants || [];
  const [selectedVariant, setSelectedVariant] = useState(variants[0] || null);

  const isManual = product.is_manual;
  const allOutOfStock = isManual && variants.every(v => v.stock_count === 0);
  const variantOutOfStock = isManual && selectedVariant && selectedVariant.stock_count === 0;
  const isHotPlayer = product.provider_key === 'hotplayer';
  const isGoldenApi = product.provider_key === 'golden_api';
  const isPromax = product.provider_key === 'promax';
  const isRedfoxx = product.provider_key === 'redfoxx';

  // Purchase modal state (shared for HotPlayer, Golden API, and Promax)
  const [showPurchaseModal, setShowPurchaseModal] = useState(false);
  const [macInput, setMacInput] = useState('');
  const [usernameInput, setUsernameInput] = useState('');
  const [passwordInput, setPasswordInput] = useState('');
  const [noteInput, setNoteInput] = useState('');
  const [modalDuration, setModalDuration] = useState('year');
  const [modalError, setModalError] = useState('');
  const [activating, setActivating] = useState(false);
  const [checkResult, setCheckResult] = useState(null);
  const [checking, setChecking] = useState(false);
  const [templates, setTemplates] = useState([]);
  const [selectedTemplateId, setSelectedTemplateId] = useState('');
  const [domains, setDomains] = useState([]);
  const [selectedDomainId, setSelectedDomainId] = useState('');
  const [bouquets, setBouquets] = useState([]);
  const [selectedBouquetId, setSelectedBouquetId] = useState('');
  const [bouquetsError, setBouquetsError] = useState('');

  const handleCheckDevice = async () => {
    const trimmedMac = macInput.trim().toUpperCase();
    if (!trimmedMac || !MAC_REGEX.test(trimmedMac)) {
      setCheckResult({ error: 'Enter a valid MAC first.' });
      return;
    }
    setChecking(true);
    setCheckResult(null);
    try {
      const { data } = await api.post('/check-device/', { mac: trimmedMac });
      setCheckResult(data);
    } catch (err) {
      const msg = err.response?.data?.error || err.message || 'Check failed';
      setCheckResult({ error: msg });
    } finally {
      setChecking(false);
    }
  };

  const handleBuy = () => {
    if (!selectedVariant) return;
    if (isHotPlayer || isGoldenApi || isPromax || isRedfoxx) {
      if (isHotPlayer) {
        const initialDuration = selectedVariant.display_name === 'Lifetime' ? 'forever' : 'year';
        setModalDuration(initialDuration);
      }
      setMacInput('');
      setUsernameInput('');
      setPasswordInput('');
      setNoteInput('');
      setModalError('');
      setCheckResult(null);
      setShowPurchaseModal(true);
      if (isGoldenApi) {
        setSelectedTemplateId('');
        setSelectedDomainId('');
        api.get('/golden-templates/', { params: { provider_id: product.provider_id } })
          .then(res => setTemplates(res.data.templates || []))
          .catch(() => setTemplates([]));
        api.get('/golden-domains/', { params: { provider_id: product.provider_id } })
          .then(res => setDomains(res.data.domains || []))
          .catch(() => setDomains([]));
      }
      if (isPromax) {
        setSelectedBouquetId('');
        setBouquetsError('');
        api.get('/promax-bouquets/', { params: { provider_id: product.provider_id } })
          .then(res => setBouquets(res.data.bouquets || []))
          .catch(() => setBouquetsError('Failed to load packages. Please try again.'));
      }
      return;
    }
    submitPurchase(selectedVariant.id, quantity);
  };

  const submitPurchase = async (variantId, qty, mac, note, username, password, templateId, domainId) => {
    setBuying(true);
    const idempotencyKey = generateUUID();
    try {
      const body = { variant_id: variantId, quantity: qty };
      if (mac) body.mac = mac;
      if (note) body.note = note;
      if (username) body.username = username;
      if (password) body.password = password;
      if (templateId) body.template_id = templateId;
      if (domainId) body.dns_domain_id = parseInt(domainId);
      const { data } = await api.post('/purchase/', body, {
        headers: { 'Idempotency-Key': idempotencyKey },
      });
      if (data.status === 'PENDING') {
        navigate(`/processing/${data.order_id}`);
      } else {
        navigate(`/receipt/${data.order_id}`);
      }
    } catch (err) {
      const msg = err.response?.data?.error || err.message || 'Purchase failed';
      if (onError) onError(msg);
      throw err;
    } finally {
      setBuying(false);
    }
  };

  const handleModalSubmit = async () => {
    setModalError('');
    setActivating(true);
    let variant = selectedVariant;

    if (isHotPlayer) {
      const trimmedMac = macInput.trim();
      if (!trimmedMac) {
        setModalError('MAC address is required.');
        setActivating(false);
        return;
      }
      if (!MAC_REGEX.test(trimmedMac)) {
        setModalError('Invalid MAC address. Use format XX:XX:XX:XX:XX:XX');
        setActivating(false);
        return;
      }
      variant = getDurationVariant(variants, modalDuration);
      if (!variant) {
        setModalError('No valid variant selected.');
        setActivating(false);
        return;
      }
      try {
        await submitPurchase(variant.id, 1, trimmedMac.toUpperCase(), noteInput.trim());
        setShowPurchaseModal(false);
      } catch (err) {
        setModalError(err.response?.data?.error || err.message || 'Activation failed');
      } finally {
        setActivating(false);
      }
    } else if (isGoldenApi) {
      try {
        await submitPurchase(variant.id, quantity, null, noteInput.trim(), usernameInput.trim(), passwordInput.trim(), selectedTemplateId, selectedDomainId);
        setShowPurchaseModal(false);
      } catch (err) {
        setModalError(err.response?.data?.error || err.message || 'Purchase failed');
      } finally {
        setActivating(false);
      }
    } else if (isPromax) {
      if (!selectedBouquetId) {
        setModalError('Please select a package.');
        setActivating(false);
        return;
      }
      try {
        await submitPurchase(variant.id, 1, null, '', '', '', selectedBouquetId, null);
        setShowPurchaseModal(false);
      } catch (err) {
        setModalError(err.response?.data?.error || err.message || 'Purchase failed');
      } finally {
        setActivating(false);
      }
    } else if (isRedfoxx) {
      try {
        await submitPurchase(variant.id, quantity, null, noteInput.trim(), usernameInput.trim(), passwordInput.trim(), null, null);
        setShowPurchaseModal(false);
      } catch (err) {
        setModalError(err.response?.data?.error || err.message || 'Purchase failed');
      } finally {
        setActivating(false);
      }
    }
  };

  const handleCancel = () => {
    setShowPurchaseModal(false);
    setMacInput('');
    setUsernameInput('');
    setPasswordInput('');
    setNoteInput('');
    setModalError('');
    setCheckResult(null);
    setBouquets([]);
    setSelectedBouquetId('');
  };

  return (
    <div className="bg-white rounded-lg shadow-md border border-gray-200 hover:shadow-lg transition-shadow overflow-hidden">
      {product.thumbnail_url && (
        <div className="aspect-video bg-gray-100 overflow-hidden">
          <img
            src={product.thumbnail_url}
            alt={product.name}
            className="w-full h-full object-cover"
            fetchpriority="high"
            loading="eager"
            width="400"
            height="300"
          />
        </div>
      )}
      <div className="p-6">
        <div className="flex items-start justify-between gap-2">
          <h3 className="text-lg font-semibold text-gray-900">{product.name}</h3>
          <div className="flex items-center gap-2">
            {allOutOfStock && (
              <span className="text-xs bg-red-100 text-red-600 px-2 py-0.5 rounded whitespace-nowrap font-medium">
                Out of Stock
              </span>
            )}
            {product.provider_name && (
              <span className="text-xs bg-gray-100 text-gray-500 px-2 py-0.5 rounded whitespace-nowrap">
                {product.provider_name}
              </span>
            )}
          </div>
        </div>
        {product.description && (
          <p className="text-sm text-gray-500 mt-1 line-clamp-2">{product.description}</p>
        )}
        <div className="mt-3 flex items-center justify-between">
          <span className="text-2xl font-bold text-indigo-600">
            {selectedVariant ? `${selectedVariant.price_in_credits}` : 'N/A'}
          </span>
          <span className="text-sm text-gray-500">credits</span>
        </div>
        {variants.length > 0 && (
          <div className="mt-4">
            <label className="text-sm text-gray-600 block mb-1">Duration:</label>
            <div className="flex flex-wrap gap-1.5">
              {variants.length === 1 ? (
                <span className={`px-3 py-1.5 text-sm font-medium rounded ${
                  isManual && variants[0].stock_count === 0
                    ? 'bg-red-50 text-red-500 line-through'
                    : 'bg-gray-100 text-gray-500'
                } cursor-default`}>
                  {variants[0].display_name || 'Lifetime'}
                </span>
              ) : (
                variants.map((v) => {
                  const hasNoStock = isManual && v.stock_count === 0;
                  return (
                    <button
                      key={v.id}
                      onClick={() => !hasNoStock && setSelectedVariant(v)}
                      disabled={hasNoStock}
                      className={`px-3 py-1.5 text-sm font-medium rounded transition-colors ${
                        hasNoStock
                          ? 'bg-red-50 text-red-400 line-through cursor-not-allowed'
                          : selectedVariant?.id === v.id
                            ? 'bg-indigo-600 text-white shadow-sm'
                            : 'bg-gray-100 text-gray-700 hover:bg-gray-200'
                      }`}
                      title={hasNoStock ? 'Out of stock' : v.display_name}
                    >
                      {v.display_name || 'Lifetime'}
                    </button>
                  );
                })
              )}
            </div>
          </div>
        )}
        {variantOutOfStock ? (
          <div className="mt-4">
            <button
              disabled
              className="w-full px-4 py-2 text-sm font-medium text-white bg-gray-400 rounded cursor-not-allowed"
            >
              Out of Stock
            </button>
          </div>
        ) : (
          <div className="mt-4 flex items-center space-x-3">
            <label className="text-sm text-gray-600">Qty:</label>
            <input
              type="number"
              min={1}
              max={50}
              value={quantity}
              onChange={(e) => setQuantity(Math.min(50, Math.max(1, parseInt(e.target.value) || 1)))}
              className="w-20 px-2 py-1 border border-gray-300 rounded text-center"
            />
            <button
              onClick={handleBuy}
              disabled={buying || !selectedVariant}
              className="flex-1 px-4 py-2 text-sm font-medium text-white bg-indigo-600 rounded hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              {buying ? 'Buying...' : 'Buy Now'}
            </button>
          </div>
        )}
      </div>

      {showPurchaseModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={handleCancel}>
          <div className="bg-white rounded-lg shadow-xl w-full max-w-md mx-4 p-6" onClick={(e) => e.stopPropagation()}>
            <h2 className="text-xl font-semibold text-gray-900 mb-4">
              {isHotPlayer ? 'Activate Device' : 'Configure Line'}
            </h2>

            <div className="space-y-4">
              {isHotPlayer && (
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">
                    MAC Address <span className="text-red-500">*</span>
                  </label>
                  <div className="flex gap-2">
                    <input
                      type="text"
                      value={macInput}
                      onChange={(e) => { setMacInput(e.target.value); setCheckResult(null); }}
                      placeholder="00:1A:79:AB:CD:EF"
                      maxLength={17}
                      className="flex-1 px-3 py-2 border border-gray-300 rounded font-mono text-sm focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500"
                    />
                    <button
                      onClick={handleCheckDevice}
                      disabled={checking}
                      className="px-3 py-2 text-sm font-medium text-indigo-700 bg-indigo-50 border border-indigo-200 rounded hover:bg-indigo-100 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                    >
                      {checking ? '...' : 'Check'}
                    </button>
                  </div>

                  {checkResult && (
                    <div className={`mt-2 p-3 rounded text-sm border ${
                      checkResult.error
                        ? 'bg-red-50 border-red-200 text-red-700'
                        : checkResult.found
                          ? 'bg-green-50 border-green-200 text-green-800'
                          : 'bg-yellow-50 border-yellow-200 text-yellow-800'
                    }`}>
                      {checkResult.error ? (
                        checkResult.error
                      ) : checkResult.found ? (
                        <div className="space-y-1">
                          <div className="flex items-center gap-2">
                            <span className={`inline-flex items-center px-2 py-0.5 rounded text-xs font-semibold ${
                              checkResult.status === 'active' ? 'bg-green-200 text-green-900' :
                              checkResult.status === 'expiring_soon' ? 'bg-yellow-200 text-yellow-900' :
                              checkResult.status === 'expired' ? 'bg-red-200 text-red-900' :
                              'bg-blue-200 text-blue-900'
                            }`}>
                              {checkResult.status === 'lifetime' ? 'Lifetime' :
                               checkResult.status === 'active' ? 'Active' :
                               checkResult.status === 'expiring_soon' ? 'Expiring Soon' :
                               checkResult.status === 'expired' ? 'Expired' : checkResult.status}
                            </span>
                            <span className="text-green-800">{checkResult.mac}</span>
                          </div>
                          <div className="text-green-700">
                            Plan: {checkResult.plan}
                            {checkResult.expires_at && <> &middot; Expires: {checkResult.expires_at}</>}
                          </div>
                        </div>
                      ) : (
                        <span>{checkResult.message || 'MAC not found on HotPlayer.'}</span>
                      )}
                    </div>
                  )}
                </div>
              )}

              {(isGoldenApi || isPromax || isRedfoxx) && (
                <>
                  <div>
                    <label className="block text-sm font-medium text-gray-700 mb-1">
                      {isPromax ? 'Duration' : 'Package'} <span className="text-red-500">*</span>
                    </label>
                    <select
                      value={selectedVariant?.id || ''}
                      onChange={(e) => {
                        const v = variants.find(x => x.id === parseInt(e.target.value));
                        if (v) setSelectedVariant(v);
                      }}
                      className="w-full px-3 py-2 border border-gray-300 rounded text-sm focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500 bg-white"
                    >
                      {variants.map((v) => (
                        <option key={v.id} value={v.id}>
                          {formatDuration(v)} ({v.price_in_credits} credits)
                        </option>
                      ))}
                    </select>
                  </div>
                </>
              )}

              {(isGoldenApi || isRedfoxx) && (
                <>
                  {isGoldenApi && templates.length > 0 && (
                    <div>
                      <label className="block text-sm font-medium text-gray-700 mb-1">
                        Template <span className="text-red-500">*</span>
                      </label>
                      <select
                        value={selectedTemplateId}
                        onChange={(e) => setSelectedTemplateId(e.target.value)}
                        className="w-full px-3 py-2 border border-gray-300 rounded text-sm focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500 bg-white"
                      >
                        <option value="">Select a template</option>
                        {templates.map((t) => (
                          <option key={t.id} value={t.id}>{t.name}</option>
                        ))}
                      </select>
                    </div>
                  )}
                  <div>
                    <label className="block text-sm font-medium text-gray-700 mb-1">
                      Username <span className="text-gray-400 font-normal">(optional, auto-generated)</span>
                    </label>
                    <input
                      type="text"
                      value={usernameInput}
                      onChange={(e) => setUsernameInput(e.target.value)}
                      placeholder="Custom username"
                      className="w-full px-3 py-2 border border-gray-300 rounded font-mono text-sm focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500"
                    />
                  </div>
                  <div>
                    <label className="block text-sm font-medium text-gray-700 mb-1">
                      Password <span className="text-gray-400 font-normal">(optional, auto-generated)</span>
                    </label>
                    <input
                      type="text"
                      value={passwordInput}
                      onChange={(e) => setPasswordInput(e.target.value)}
                      placeholder="Custom password"
                      className="w-full px-3 py-2 border border-gray-300 rounded font-mono text-sm focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500"
                    />
                  </div>
                </>
              )}

              {isPromax && bouquets.length > 0 && (
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">
                    Package <span className="text-red-500">*</span>
                  </label>
                  <select
                    value={selectedBouquetId}
                    onChange={(e) => setSelectedBouquetId(e.target.value)}
                    className="w-full px-3 py-2 border border-gray-300 rounded text-sm focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500 bg-white"
                  >
                    <option value="">Select a package</option>
                    {bouquets.map((b) => (
                      <option key={b.id} value={b.id}>{b.name}</option>
                    ))}
                  </select>
                </div>
              )}

              {isPromax && bouquetsError && !bouquets.length && (
                <div className="p-3 bg-red-50 border border-red-200 rounded text-sm text-red-700">
                  {bouquetsError}
                </div>
              )}

              {isHotPlayer && (
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">Subscription</label>
                  <div className="flex gap-2">
                    <button
                      onClick={() => setModalDuration('year')}
                      className={`flex-1 px-3 py-2 text-sm font-medium rounded transition-colors ${
                        modalDuration === 'year'
                          ? 'bg-indigo-600 text-white shadow-sm'
                          : 'bg-gray-100 text-gray-700 hover:bg-gray-200'
                      }`}
                    >
                      1 YEAR (1 CREDIT)
                    </button>
                    <button
                      onClick={() => setModalDuration('forever')}
                      className={`flex-1 px-3 py-2 text-sm font-medium rounded transition-colors ${
                        modalDuration === 'forever'
                          ? 'bg-indigo-600 text-white shadow-sm'
                          : 'bg-gray-100 text-gray-700 hover:bg-gray-200'
                      }`}
                    >
                      FOREVER (2.5 CREDITS)
                    </button>
                  </div>
                </div>
              )}

              {!isPromax && (
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">
                    Note <span className="text-gray-400 font-normal">(optional)</span>
                  </label>
                  <textarea
                    value={noteInput}
                    onChange={(e) => setNoteInput(e.target.value)}
                    placeholder="Client: John's Firestick"
                    rows={2}
                    className="w-full px-3 py-2 border border-gray-300 rounded text-sm focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500"
                  />
                </div>
              )}
            </div>

            {modalError && (
              <div className="mt-4 p-3 bg-red-50 border border-red-200 rounded text-sm text-red-700">
                {modalError}
              </div>
            )}

            <div className="mt-6 flex gap-3">
              <button
                onClick={handleModalSubmit}
                disabled={activating}
                className="flex-1 px-4 py-2 text-sm font-medium text-white bg-indigo-600 rounded hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
              >
                {activating ? 'Processing...' : (isHotPlayer ? 'Activate' : 'Purchase')}
              </button>
              <button
                onClick={handleCancel}
                disabled={activating}
                className="flex-1 px-4 py-2 text-sm font-medium text-gray-700 bg-gray-100 rounded hover:bg-gray-200 transition-colors"
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
