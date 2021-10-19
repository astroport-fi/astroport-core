import path from 'path';
import CleanWebpackPlugin from 'clean-webpack-plugin';
import CopyWebpackPlugin from 'copy-webpack-plugin';
import HtmlWebpackPlugin from 'html-webpack-plugin';

const config = {
    target: 'web',
    entry: './migrate_liquidity.ts',
    mode: 'development',
    module: {
        rules: [
            {
                test: /\.ts?$/,
                loader: "ts-loader",
                exclude: /node_modules/,
                include: path.resolve(path.dirname('scripts'))
            }
        ]
    },
    output: {
        filename: '[name]bundle.js',
        path: '/home/kid/atticlab/astroport/scripts'
    },
    resolve: {
        extensions: ['.tsx', '.ts', '.jsx', '.js'],
        fallback: {
            "fs": false,
            "path": false,
            "stream": false
        },
    },
    plugins: [
        new CleanWebpackPlugin.CleanWebpackPlugin(),
        new CopyWebpackPlugin({
            patterns: [
                {
                    from: '**/*',
                    context: path.resolve(path.dirname('scripts')),
                    to: './assets',
                },
            ],
        }),
        new HtmlWebpackPlugin({
            template: 'public/index.html',
            filename: 'index.html',
            minify: {
                collapseWhitespace: true,
                removeComments: true,
                removeRedundantAttributes: true,
                useShortDoctype: true,
            },
        }),
    ]
};

export default config;