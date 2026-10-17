# Railway backend

Deploy Mahin2076/silico, branch main, using the Dockerfile builder and Dockerfile.railway. New Railway services no longer accept legacy railway.json configuration; set the builder, Dockerfile path, /health check, and single replica in Railway service settings. Keep the frontend on Vercel. Attach one persistent volume at /data before starting the service. Run one replica. Railway supplies RAILWAY_VOLUME_MOUNT_PATH; startup refuses to run without that directory.

Enable GitHub auto deploy for main in the service settings.

Set server-only variables in Railway: CRUSOE_API_KEY, CRUSOE_MODEL, CRUSOE_REASONING_EFFORT, SILICO_VERIFIER_MODEL, and YOU_API_KEY (use the configured provider variable name). Never commit values. Leave legacy provider credentials and NEWS_API_KEY unset to avoid unbudgeted activity.

The public PORT serves the allowlisted Python gateway. Rust runs internally on port 8086. Expose only the gateway port when generating the Railway domain. The volume holds cache.db, state.db, experiments.db, and public-demo-admissions.db. Do not delete the volume or ledger: that would reset the global four-run admission cap.

Before cutting over, freeze local new live admissions and migrate its existing public-demo-admissions.db to the volume (and experiment state if any runs exist). An empty ledger is safe only when zero previous public admissions are verified and the old ingress is disabled before the cloud service accepts live requests. Do not allow separate local and cloud spending pools simultaneously.

Verify /health and /experiments/config, a free sample experiment, and a restart with persisted results. Then replace the Vercel /api rewrite destination with the generated Railway HTTPS domain, preserving the path. Do not remove the old connection until cloud validation passes. Do not describe this as deployed until the cloud URL works.

Trial memory and outbound API access must be verified in the Railway account. Hosting costs are separate from the API budget. No paid upgrade is authorized by this configuration.
