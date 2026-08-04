import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import CopyButton from './CopyButton';
import { formatCredentialBlockFromSchema, downloadTextFile, sanitizeFilename } from '../lib/helpers';

export default function CredentialCard({ credential, index }) {
  const navigate = useNavigate();
  const {
    credential_data: credData,
    provider_config: providerConfig,
    expires_at: expiresAt,
    provider_adapter_key: providerKey,
    // Legacy fallback fields
    username,
    password,
    dns_domain: dns,
    m3u_url: m3uUrl,
  } = credential;

  const fields = providerConfig?.fields;
  const hasSchema = fields && fields.length > 0;

  // State for show/hide toggles on secret fields (keyed by field key)
  const [visibleSecrets, setVisibleSecrets] = useState({});

  const toggleSecret = (key) => {
    setVisibleSecrets((prev) => ({ ...prev, [key]: !prev[key] }));
  };

  // Build the text block for Copy All / Download
  const block = hasSchema
    ? formatCredentialBlockFromSchema(fields, credData)
    : `Username: ${username}\nPassword: ${password}\nDNS: ${dns}${m3uUrl ? `\nM3U URL: ${m3uUrl}` : ''}`;

  const handleCopyAll = async () => {
    await navigator.clipboard.writeText(block);
  };

  const handleDownload = () => {
    const label = credData?.username || credData?.mac || username || `credential-${index + 1}`;
    const safeName = sanitizeFilename(`credential-${label}.txt`);
    downloadTextFile(block, safeName);
  };

  // --- Dynamic field renderer ---
  const renderField = (field) => {
    const value = credData?.[field.key] ?? '';
    const copyable = field.copyable !== false;

    if (field.type === 'secret') {
      const isVisible = visibleSecrets[field.key];
      return (
        <div key={field.key} className="flex items-center justify-between">
          <div className="flex-1 min-w-0 mr-2">
            <span className="text-xs text-gray-500">{field.label?.toUpperCase()}</span>
            <p className="text-sm font-mono text-gray-900">
              {isVisible ? value : '••••••••'}
            </p>
          </div>
          <div className="flex space-x-2">
            <button
              onClick={() => toggleSecret(field.key)}
              className="px-3 py-1 text-sm font-medium text-gray-700 bg-gray-100 rounded hover:bg-gray-200 transition-colors"
            >
              {isVisible ? 'Hide' : 'Show'}
            </button>
            {copyable && <CopyButton text={value} label="Copy" />}
          </div>
        </div>
      );
    }

    if (field.type === 'url') {
      return (
        <div key={field.key} className="flex items-center justify-between">
          <div className="flex-1 min-w-0 mr-2">
            <span className="text-xs text-gray-500">{field.label?.toUpperCase()}</span>
            <p className="text-sm font-mono text-gray-900 break-all">
              <a href={value} target="_blank" rel="noopener noreferrer" className="text-indigo-600 hover:text-indigo-800 underline">
                {value}
              </a>
            </p>
          </div>
          {copyable && <CopyButton text={value} label="Copy" />}
        </div>
      );
    }

    return (
      <div key={field.key} className="flex items-center justify-between">
        <div className="flex-1 min-w-0 mr-2">
          <span className="text-xs text-gray-500">{field.label?.toUpperCase()}</span>
          <p className="text-sm font-mono text-gray-900 break-all">{value}</p>
        </div>
        {copyable && <CopyButton text={String(value)} label="Copy" />}
      </div>
    );
  };

  // --- Legacy fallback renderer (no schema) ---
  const renderLegacy = () => (
    <>
      <div className="flex items-center justify-between">
        <div>
          <span className="text-xs text-gray-500">USERNAME</span>
          <p className="text-sm font-mono text-gray-900">{username}</p>
        </div>
        <CopyButton text={username} label="Copy" />
      </div>
      {password && (
        <div className="flex items-center justify-between">
          <div>
            <span className="text-xs text-gray-500">PASSWORD</span>
            <p className="text-sm font-mono text-gray-900">
              {visibleSecrets._legacy_pw ? password : '••••••••'}
            </p>
          </div>
          <div className="flex space-x-2">
            <button
              onClick={() => toggleSecret('_legacy_pw')}
              className="px-3 py-1 text-sm font-medium text-gray-700 bg-gray-100 rounded hover:bg-gray-200 transition-colors"
            >
              {visibleSecrets._legacy_pw ? 'Hide' : 'Show'}
            </button>
            <CopyButton text={password} label="Copy" />
          </div>
        </div>
      )}
      {dns && (
        <div className="flex items-center justify-between">
          <div className="flex-1 min-w-0 mr-2">
            <span className="text-xs text-gray-500">DNS</span>
            <p className="text-sm font-mono text-gray-900 truncate">{dns}</p>
          </div>
          <CopyButton text={dns} label="Copy" />
        </div>
      )}
      {m3uUrl && (
        <div className="flex items-center justify-between">
          <div className="flex-1 min-w-0 mr-2">
            <span className="text-xs text-gray-500">M3U URL</span>
            <p className="text-sm font-mono text-gray-900 break-all">
              <a href={m3uUrl} target="_blank" rel="noopener noreferrer" className="text-indigo-600 hover:text-indigo-800 underline">
                {m3uUrl}
              </a>
            </p>
          </div>
          <CopyButton text={m3uUrl} label="Copy" />
        </div>
      )}
    </>
  );

  return (
    <div className="bg-white rounded-lg shadow border border-gray-200 p-4">
      <div className="flex items-center justify-between mb-3">
        <span className="text-sm font-medium text-gray-500">#{index + 1}</span>
        <span className="text-xs text-gray-400">
          {providerConfig?.expiry_label || 'Expires'}:{' '}
          {expiresAt ? new Date(expiresAt).toLocaleDateString() : 'Lifetime'}
        </span>
      </div>
      <div className="space-y-2">
        {hasSchema ? fields.map(renderField) : renderLegacy()}
      </div>
      <div className="mt-4 flex flex-col sm:flex-row gap-2.5">
        <button
          onClick={handleCopyAll}
          className="flex-1 px-3 py-2 text-sm font-medium text-white bg-green-600 rounded hover:bg-green-700 transition-colors"
        >
          Copy All
        </button>
        <button
          onClick={handleDownload}
          className="flex-1 px-3 py-2 text-sm font-medium text-white bg-blue-600 rounded hover:bg-blue-700 transition-colors"
        >
          Download .txt
        </button>
      </div>
    </div>
  );
}
