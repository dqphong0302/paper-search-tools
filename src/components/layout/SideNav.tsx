import React from 'react';
import {
  Search,
  Activity,
  Bookmark,
  Settings,
  PanelLeftClose,
  PanelLeftOpen,
  Layers,
} from 'lucide-react';

export type TabType = 'search' | 'research' | 'gateway' | 'settings';

interface NavItem {
  id: TabType;
  label: string;
  icon: React.ReactNode;
  badge?: string;
  badgeColor?: string;
}

interface SideNavProps {
  activeTab: TabType;
  setActiveTab: (tab: TabType) => void;
  isCollapsed: boolean;
  setIsCollapsed: (val: boolean | ((prev: boolean) => boolean)) => void;
  savedCount: number;
  downloadCount?: number;
}

export const SideNav: React.FC<SideNavProps> = ({
  activeTab,
  setActiveTab,
  isCollapsed,
  setIsCollapsed,
  savedCount,
  downloadCount = 0,
}) => {
  const researchCount = savedCount + downloadCount;
  const mainNav: NavItem[] = [
    { id: 'search', label: 'Search', icon: <Search size={17} /> },
    {
      id: 'research',
      label: 'Library',
      icon: <Bookmark size={17} />,
      badge: researchCount > 0 ? `${researchCount}` : undefined,
      badgeColor: 'badge-cyan',
    },
    {
      id: 'gateway',
      label: 'Connections',
      icon: <Activity size={17} />,
    },
  ];

  const renderNavGroup = (title: string, items: NavItem[]) => (
    <div className="nav-group">
      {!isCollapsed && title && (
        <div
          className="nav-group-title"
          style={{
            fontSize: '10px',
            fontWeight: 600,
            color: 'var(--text-dim)',
            padding: '3px 10px',
            marginBottom: 2,
            letterSpacing: '0.05em',
          }}
        >
          {title}
        </div>
      )}
      {items.map((item) => {
        const isActive = activeTab === item.id;
        return (
          <button
            id={`nav-${item.id}`}
            key={item.id}
            className={`nav-item ${isActive ? 'active' : ''}`}
            onClick={() => setActiveTab(item.id)}
            title={isCollapsed ? item.label : undefined}
            aria-current={isActive ? 'page' : undefined}
            aria-label={item.label}
          >
            <div className="nav-item-icon">{item.icon}</div>
            {!isCollapsed && (
              <>
                <span className="nav-item-text">{item.label}</span>
                {item.badge && (
                  <span
                    className={`cockpit-badge ${item.badgeColor || 'badge-cyan'}`}
                    style={{ marginLeft: 'auto', fontSize: '9px', padding: '1px 5px' }}
                  >
                    {item.badge}
                  </span>
                )}
              </>
            )}
          </button>
        );
      })}
    </div>
  );

  return (
    <aside className={`side-nav ${isCollapsed ? 'collapsed' : ''}`}>
      {/* Brand Header */}
      <div className="side-nav-brand">
        <div className="brand-logo-box" title="ScholarGateway">
          <Layers size={17} />
        </div>
        {!isCollapsed && (
          <div className="brand-text-container">
            <div className="brand-app-name">ScholarGateway</div>
            <div className="brand-app-sub">Local · Desktop</div>
          </div>
        )}
      </div>

      {/* Navigation Sections */}
      <nav className="nav-section">
        {renderNavGroup('', mainNav)}
      </nav>

      {/* Bottom Controls */}
      <div className="nav-bottom">
        {renderNavGroup('', [{ id: 'settings', label: 'Settings', icon: <Settings size={17} /> }])}
        {/* The gateway status lives in the top bar; a second copy here only
            repeated the same state in every screen. */}

        <button
          id="toggle-sidebar"
          className="collapse-toggle-btn"
          onClick={() => setIsCollapsed((prev) => !prev)}
          title={isCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
          style={{
            justifyContent: isCollapsed ? 'center' : 'flex-start',
            width: '100%',
          }}
        >
          {isCollapsed ? <PanelLeftOpen size={15} /> : <PanelLeftClose size={15} />}
          {!isCollapsed && <span>Collapse</span>}
        </button>
      </div>
    </aside>
  );
};
