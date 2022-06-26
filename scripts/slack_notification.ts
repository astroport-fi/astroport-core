// @ts-ignore
import SlackNotify from 'slack-notify';

const MY_SLACK_WEBHOOK_URL = 'https://hooks.slack.com/services/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';
const slack = SlackNotify(MY_SLACK_WEBHOOK_URL);

export async function sendNotification(msg: string) {
    slack.send(msg)
        .then(() => {
            console.log('done!');
        }).catch((err: any) => {
        console.error(err);
    });
}