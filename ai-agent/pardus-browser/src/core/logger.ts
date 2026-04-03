import pino from 'pino';

const isJson = process.env.LOG_FORMAT === 'json';

export const logger = pino({
  name: 'pardus-agent',
  level: process.env.LOG_LEVEL || 'info',
  ...(isJson
    ? {}
    : {
        transport: {
          target: 'pino-pretty',
          options: {
            colorize: true,
            translateTime: 'SYS:HH:MM:ss.l',
            ignore: 'pid,hostname',
          },
        },
      }),
});
