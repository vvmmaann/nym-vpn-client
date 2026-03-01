import { GatewaysProvider } from '../gateways';
import { useMainState } from '../main';
import { NodeListStateProvider } from '../node-list-state';
import { Socks5Provider } from '../socks5';
import { TrayProvider } from '../tray';

function SystemAuthenticationProvider({
  children,
}: {
  children: React.ReactNode;
}) {
  const { daemonStatus } = useMainState();

  if (daemonStatus === 'auth-denied') {
    return <>{children}</>;
  }

  return (
    <GatewaysProvider>
      <TrayProvider>
        <NodeListStateProvider>
          <Socks5Provider>{children}</Socks5Provider>
        </NodeListStateProvider>
      </TrayProvider>
    </GatewaysProvider>
  );
}

export default SystemAuthenticationProvider;
