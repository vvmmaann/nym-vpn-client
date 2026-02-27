import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router';
import { Button, PageAnim } from '../../ui';
import { useMainState } from '../../contexts';
import { routes } from '../../router';

function SystemAuthentication() {
  const navigate = useNavigate();
  const { t } = useTranslation('systemAuthentication');

  const { daemonStatus } = useMainState();
  console.log('[SystemAuthentication] daemonStatus', daemonStatus);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    console.log('[SystemAuthentication] useEffect', daemonStatus);
    if (daemonStatus !== 'auth-denied') {
      console.log(
        '[SystemAuthentication] daemonStatus is not auth-denied, navigating to root',
      );
      navigate(routes.root, { replace: true });
    }
  }, [daemonStatus, navigate]);

  const handleAuthenticate = async () => {
    setLoading(true);
    try {
      await invoke('retry_authentication');
    } catch (e: unknown) {
      console.error('retry_authentication failed', e);
    } finally {
      setLoading(false);
    }
  };

  return (
    <PageAnim className="h-full flex flex-col justify-end items-center gap-8 select-none cursor-default">
      <section className="embla w-full h-full flex flex-col justify-between">
        <h1>{t('heading')}</h1>
        <p>{t('description')}</p>
        <Button onClick={handleAuthenticate} spinner={loading}>
          {t('authenticate-button')}
        </Button>
      </section>
    </PageAnim>
  );
}

export default SystemAuthentication;
