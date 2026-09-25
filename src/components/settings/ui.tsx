import React, { useState } from 'react';
import { Eye, EyeOff } from 'lucide-react';
import { CREDENTIAL_LABELS, CredentialMeta, isConfiguredSecret } from './model';

export const Card: React.FC<{
  title?: string;
  subtitle?: string;
  icon?: React.ReactNode;
  right?: React.ReactNode;
  children: React.ReactNode;
  advanced?: boolean;
}> = ({ title, subtitle, icon, right, children, advanced }) => advanced ? (
  <details className="compact-options">
    <summary>{title}</summary>
    <div className="compact-options-body">
      {subtitle && <p className="page-subtitle">{subtitle}</p>}
      {right}
      {children}
    </div>
  </details>
) : (
  <section className="cockpit-card settings-card">
    {(title || right) && (
      <header className="settings-card-header">
        <div className="settings-card-heading">
          <div className="settings-card-title">
            {icon}
            <span>{title}</span>
          </div>
          {subtitle && <div className="settings-card-subtitle">{subtitle}</div>}
        </div>
        {right}
      </header>
    )}
    {children}
  </section>
);

/** Label + control stacked, the layout every settings field uses. */
export const Field: React.FC<{ label: string; children: React.ReactNode }> = ({ label, children }) => (
  <label className="settings-field">
    <span className="field-label">{label}</span>
    {children}
  </label>
);

interface TextFieldProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'onChange'> {
  label: string;
  onValue: (value: string) => void;
}

export const TextField: React.FC<TextFieldProps> = ({ label, onValue, type = 'text', className, ...rest }) => (
  <Field label={label}>
    <input type={type} className={`settings-input ${className ?? ''}`} onChange={(e) => onValue(e.target.value)} {...rest} />
  </Field>
);

export const CheckboxField: React.FC<{ id?: string; label: string; checked: boolean; onChange: (checked: boolean) => void }> = ({ id, label, checked, onChange }) => (
  <label className="check-label settings-check">
    <input id={id} type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
    <span>{label}</span>
  </label>
);

/**
 * A credential input. Stored secrets never reach the UI: the field shows the
 * keep-sentinel as "Saved", and typing replaces it.
 */
export const SecretField: React.FC<{ name: string; value: string; onChange: (name: string, value: string) => void; placeholder?: string }> = ({ name, value, onChange, placeholder }) => {
  const [visible, setVisible] = useState(false);
  const meta: CredentialMeta = CREDENTIAL_LABELS[name] ?? { label: name, secret: true };
  const isSecret = meta.secret !== false;
  const saved = isConfiguredSecret(value);
  return (
    <label className="settings-field">
      <span className="field-label">{meta.label}</span>
      <div className="secret-input">
        <input
          id={`setting-${name}`}
          type={isSecret && !visible ? 'password' : (meta.type ?? 'text')}
          autoComplete="off"
          className={`settings-input ${isSecret ? 'has-toggle' : ''}`}
          placeholder={saved ? 'Saved — enter to overwrite' : (placeholder ?? meta.placeholder ?? '')}
          value={value}
          onChange={(e) => onChange(name, e.target.value)}
        />
        {isSecret && (
          <button id={`toggle-${name}-visibility`} type="button" className="secret-toggle" onClick={() => setVisible((v) => !v)} aria-label={`Toggle ${meta.label} visibility`}>
            {visible ? <EyeOff size={14} /> : <Eye size={14} />}
          </button>
        )}
      </div>
      {saved && <span className="secret-saved">Saved securely</span>}
    </label>
  );
};

export interface TestResult {
  loading: boolean;
  success?: boolean;
  message?: string;
  latency?: number;
}

export const TestStatus: React.FC<{ status?: TestResult }> = ({ status }) => {
  if (!status) return null;
  const tone = status.loading ? 'pending' : status.success ? 'ok' : 'fail';
  return (
    <div className={`test-status ${tone}`}>
      {status.loading ? 'Testing…' : `${status.message ?? ''}${status.latency ? ` (${status.latency}ms)` : ''}`}
    </div>
  );
};
