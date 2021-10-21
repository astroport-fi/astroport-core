const path = require("path");
const webpack = require("webpack");

module.exports = {
    target: "web",
    mode: "development",
    entry: ['@babel/polyfill', './src/migrate_liquidity.ts'],

    output: {
        path: path.resolve(__dirname, "./public"),
        filename: 'bundle-[name].js',
        publicPath: 'public'
    },
    resolve: {
        extensions: ['', '.js', '.ts'],
        fallback: {
            "buffer": require.resolve("buffer/"),
            "events": require.resolve("events/"),
            "stream": require.resolve("stream-browserify/")
        },
    },
    module: {
        rules: [
            {
                test: /\.ts$/,
                use: ['babel-loader', 'ts-loader'],
                exclude: /node_modules/,
            }
        ],
    },
    plugins: [
        // Work around for Buffer is undefined:
        // https://github.com/webpack/changelog-v5/issues/10
        new webpack.ProvidePlugin({
            Buffer: ['buffer', 'Buffer'],
        }),
    ],
};